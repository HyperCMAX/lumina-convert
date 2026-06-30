#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::panic;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use once_cell::sync::Lazy;
use thread_priority::{ThreadPriority, set_current_thread_priority};
use std::convert::TryInto;
use serde::Deserialize;
use serde_json::json;
use tauri::{Emitter, AppHandle, Manager};
use walkdir::WalkDir;

use libvips::{VipsApp, VipsImage, ops};

// 🚨 终极魔法：直接调用 libvips C 库底层 API，动态修改其内部线程池大小
extern "C" {
    fn vips_concurrency_set(concurrency: i32);
}

static RAYON_POOL: Lazy<rayon::ThreadPool> = Lazy::new(|| {
    ThreadPoolBuilder::new()
        .num_threads(num_cpus::get())
        .start_handler(|_id| {
            let _ = set_current_thread_priority(ThreadPriority::Crossplatform(
                10.try_into().unwrap()
            ));
        })
        .build()
        .expect("致命错误: 无法初始化 Rayon 低优先级线程池")
});

static CURRENT_TOKEN: Lazy<Mutex<Option<Arc<AtomicBool>>>> = Lazy::new(|| Mutex::new(None));

#[derive(Deserialize, Clone)]
pub struct ConvertOptions {
    pub format: String,
    pub preset: String,
    pub metadata: String,
    pub output_dir: String,
    pub resize_mode: String,
    pub resize_value: u32,
    pub bit_depth: String,
    pub rename_template: String,
    pub concurrency: u32,
    pub perf_mode: String,
}

fn get_safe_output_path(dir: &Path, stem: &str, original_ext: &str, preset: &str, ext: &str) -> PathBuf {
    let base_name = format!("{}_{}_{}", stem, original_ext, preset);
    let mut candidate = dir.join(format!("{}.{}", base_name, ext));
    let mut counter = 1;
    while candidate.exists() {
        candidate = dir.join(format!("{} ({}).{}", base_name, counter, ext));
        counter += 1;
        if counter > 9999 { break; }
    }
    candidate
}

fn convert_single_image(path: &str, options: &ConvertOptions, counter: usize) -> Result<String, String> {
    if path.contains("error") {
        panic!("模拟底层崩溃！");
    }

    let path_obj = Path::new(path);
    let file_stem = path_obj.file_stem().unwrap_or_default().to_string_lossy();
    let original_ext = path_obj.extension().unwrap_or_default().to_string_lossy().to_lowercase();

    let ext = match options.format.as_str() {
        "jpeg" => "jpg",
        "heic" => "webp",
        _ => options.format.as_str()
    };

    // 🚨 双保险解码：先试文件读取（格式检测最准），失败再回退内存解码（免疫中文路径）
    let img = VipsImage::new_from_file(path)
        .or_else(|_e1| {
            let data = std::fs::read(path).map_err(|e| format!("读取文件失败: {}", e))?;
            VipsImage::new_from_buffer(&data, "")
                .map_err(|e2| {
                    let err_str = format!("{:?}", e2).to_lowercase();
                    if (original_ext == "hif" || original_ext == "heic" || original_ext == "heif")
                        && (err_str.contains("no loader") || err_str.contains("heif"))
                    {
                        "⚠️ Windows 环境缺失 HEVC 解码组件 (H.265 专利壁垒)。请确保打包时已注入 libheif.dll，或在微软商店安装 'HEVC 视频扩展'。".to_string()
                    } else {
                        format!("硬件解码失败(可能为损坏文件): {:?}", e2)
                    }
                })
        })?;

    let mut proc = ops::autorot(&img)
        .map_err(|e| format!("自动旋转失败: {:?}", e))?;

    // 🚨 引擎 1: 智能尺寸 (Lanczos3 重采样)
    if options.resize_mode != "none" && options.resize_value > 0 {
        let (w, h) = (proc.get_width() as f64, proc.get_height() as f64);
        let scale = match options.resize_mode.as_str() {
            "fit" => {
                let max_side = w.max(h);
                if (options.resize_value as f64) < max_side {
                    Some(options.resize_value as f64 / max_side)
                } else { None }
            },
            "percent" if options.resize_value != 100 => {
                Some(options.resize_value as f64 / 100.0)
            },
            _ => None,
        };
        if let Some(s) = scale {
            let opts = ops::ResizeOptions {
                kernel: ops::Kernel::Lanczos3,
                ..Default::default()
            };
            proc = match ops::resize_with_opts(&proc, s, &opts) {
                Ok(img) => img,
                Err(_) => proc,
            };
        }
    }

    // 🚨 引擎 2: 16-bit / HDR 色彩映射
    if let "jpeg" | "webp" | "avif" = options.format.as_str() {
        proc = match ops::colourspace(&proc, ops::Interpretation::Srgb) {
            Ok(img) => img,
            Err(_) => proc,
        };
    }
    if options.bit_depth == "16bit" {
        if let "png" | "tiff" = options.format.as_str() {
            proc = match ops::cast(&proc, ops::BandFormat::Ushort) {
                Ok(img) => img,
                Err(_) => proc,
            };
        }
    } else if options.bit_depth == "8bit" {
        proc = match ops::cast(&proc, ops::BandFormat::Uchar) {
            Ok(img) => img,
            Err(_) => proc,
        };
    }

    // 🚨 引擎 3: 模板化批量重命名
    let final_stem = options.rename_template
        .replace("{original}", &file_stem)
        .replace("{width}", &proc.get_width().to_string())
        .replace("{height}", &proc.get_height().to_string())
        .replace("{ext}", &original_ext)
        .replace("{counter}", &format!("{:04}", counter));

    let out_path = get_safe_output_path(
        Path::new(&options.output_dir),
        &final_stem,
        &original_ext,
        &options.preset,
        ext
    );

    let mut meta_params = vec![];
    if options.metadata == "strip" { meta_params.push("strip".to_string()); }
    else if options.metadata == "icc" { meta_params.push("keep=icc".to_string()); }

    let save_options = match options.format.as_str() {
        "jpeg" => {
            let q = match options.preset.as_str() { "fast" => 60, "standard" => 80, "high" => 95, "lossless" => 100, _ => 80 };
            let mut params = vec![format!("Q={}", q), "optimize_coding".to_string()];
            params.extend(meta_params);
            format!(".jpg[{}]", params.join(","))
        },
        "png" => {
            let compression = match options.preset.as_str() { "fast" => 1, "standard" => 6, "high" => 9, "lossless" => 9, _ => 6 };
            let mut params = vec![format!("compression={}", compression)];
            params.extend(meta_params);
            format!(".png[{}]", params.join(","))
        },
        "webp" | "heic" => {
            let mut params = if options.preset == "lossless" {
                vec!["lossless".to_string()]
            } else {
                let q = match options.preset.as_str() { "fast" => 60, "standard" => 80, "high" => 95, _ => 80 };
                vec![format!("Q={}", q)]
            };
            params.extend(meta_params);
            format!(".webp[{}]", params.join(","))
        },
        "avif" => {
            let q = match options.preset.as_str() { "fast" => 50, "standard" => 70, "high" => 90, "lossless" => 100, _ => 70 };
            let mut params = if options.preset == "lossless" {
                vec!["lossless".to_string()]
            } else {
                vec![format!("Q={}", q)]
            };
            params.extend(meta_params);
            format!(".avif[{}]", params.join(","))
        },
        "jxl" => {
            let q = match options.preset.as_str() { "fast" => 70, "standard" => 85, "high" => 95, "lossless" => 100, _ => 85 };
            let mut params = if options.preset == "lossless" {
                vec!["lossless".to_string()]
            } else {
                vec![format!("Q={}", q)]
            };
            params.extend(meta_params);
            format!(".jxl[{}]", params.join(","))
        },
        "tiff" => {
            let compression = if options.preset == "fast" { "none" } else { "lzw" };
            let mut params = vec![format!("compression={}", compression)];
            params.extend(meta_params);
            format!(".tiff[{}]", params.join(","))
        },
        "bmp" => format!(".bmp[{}]", meta_params.join(",")),
        "gif" => format!(".gif[{}]", meta_params.join(",")),
        _ => return Err("不支持的格式".to_string())
    };

    let base_path = out_path.with_extension("");
    let final_path_str = format!("{}{}", base_path.to_string_lossy(), save_options);

    proc.image_write_to_file(&final_path_str)
        .map_err(|e| format!("编码失败: {:?}", e))?;

    Ok(format!("成功: {} -> {}", path_obj.file_name().unwrap().to_string_lossy(), out_path.file_name().unwrap().to_string_lossy()))
}

#[tauri::command]
async fn scan_folder(path: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let mut images = Vec::new();
        let valid_exts = ["jpg", "jpeg", "png", "heic", "heif", "hif", "webp", "avif", "jxl", "tiff", "tif", "bmp", "gif", "svg", "pdf"];
        
        for entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
            if images.len() >= 5000 { break; } 
            
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    if valid_exts.contains(&ext_str.as_str()) {
                        images.push(entry.path().to_string_lossy().into_owned());
                    }
                }
            }
        }
        Ok(images)
    }).await.unwrap_or(Err("扫描任务崩溃".to_string()))
}

#[tauri::command]
fn cancel_conversion() {
    if let Some(token) = CURRENT_TOKEN.lock().unwrap().as_ref() {
        token.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
fn open_output_dir(path: String) -> Result<(), String> {
    open::that(path).map_err(|e| format!("打开目录失败: {}", e))
}

#[tauri::command]
async fn process_images(paths: Vec<String>, options: ConvertOptions, app: AppHandle) -> Result<String, String> {
    let cancel_token = Arc::new(AtomicBool::new(false));
    let rename_counter = Arc::new(AtomicUsize::new(1));
    let completed_count = Arc::new(AtomicUsize::new(0));

    *CURRENT_TOKEN.lock().unwrap() = Some(cancel_token.clone());

    let out_dir_path = Path::new(&options.output_dir);
    if !out_dir_path.exists() || !out_dir_path.is_dir() {
        return Err("致命错误: 导出目录不存在或不是有效文件夹".to_string());
    }

    let total_files = paths.len();

    let app_handle = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        let cpu_cores = num_cpus::get();
        let (max_tasks, vips_threads) = match options.perf_mode.as_str() {
            "background" => (2.max(options.concurrency as usize), 1),
            "balanced" => {
                let half = (cpu_cores / 2).max(2);
                (if options.concurrency > 0 { options.concurrency as usize } else { half }, half as i32)
            },
            _ => {
                (if options.concurrency > 0 { options.concurrency as usize } else { cpu_cores }, cpu_cores as i32)
            }
        };

        // 🚨 终极安全：在单线程上下文中配置 libvips C 库全局状态 (只执行一次，绝不污染并发环境)
        unsafe { vips_concurrency_set(vips_threads); }

        let (tx, rx) = flume::bounded(max_tasks);
        for _ in 0..max_tasks { tx.send(()).unwrap(); }
        let rx = Arc::new(rx);
        let tx = Arc::new(tx);

        let results: Vec<Result<String, String>> = RAYON_POOL.install(|| {
            paths
                .into_par_iter()
                .map(|path| {
                    if cancel_token.load(Ordering::Relaxed) { return Err("用户取消".to_string()); }
                    
                    let _token = rx.recv().unwrap(); 
                    let current_counter = rename_counter.fetch_add(1, Ordering::SeqCst);
                    
                    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                        convert_single_image(&path, &options, current_counter)
                    }));
                    let _ = tx.send(()); 
                    
                    let done = completed_count.fetch_add(1, Ordering::SeqCst) + 1;
                    let filename = Path::new(&path).file_name().unwrap_or_default().to_string_lossy().to_string();
                    let is_success = result.is_ok() && result.as_ref().unwrap().is_ok();

                    let _ = app_handle.emit("convert-progress", json!({
                        "completed": done,
                        "total": total_files,
                        "filename": filename,
                        "status": if is_success { "success" } else { "failed" }
                    }));

                    match result {
                        Ok(Ok(msg)) => Ok(msg),
                        Ok(Err(e)) => Err(format!("处理失败 {}: {}", path, e)),
                        Err(_) => Err(format!("严重错误: 文件 {} 导致底层崩溃", path)),
                    }
                })
                .collect()
        });

        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let fail_count = results.len() - success_count;
        let canceled = cancel_token.load(Ordering::Relaxed);

        // 🚨 任务完成，发送系统原生通知事件
        let _ = app_handle.emit("conversion-finished", json!({
            "success": success_count,
            "failed": fail_count
        }));

        if canceled { format!("任务已安全终止！成功: {} 张", success_count) } 
        else { format!("批量处理完成！成功: {} 张，失败: {} 张", success_count, fail_count) }
    }).await.map_err(|e| format!("任务调度崩溃: {}", e))?;

    *CURRENT_TOKEN.lock().unwrap() = None;
    Ok(result)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 🚨 核心截胡：保留目录结构，精准引导 libvips 解码模块
            if let Ok(resource_dir) = app.path().resource_dir() {
                let vips_bin = resource_dir.join("vips").join("bin");
                let vips_lib = resource_dir.join("vips").join("lib");

                if vips_bin.exists() && vips_lib.exists() {
                    let bin_str = vips_bin.to_string_lossy().to_string();
                    let lib_str = vips_lib.to_string_lossy().to_string();

                    // 1. 注入 PATH (让系统能找到 libvips-42.dll 和 libheif.dll)
                    let current_path = std::env::var("PATH").unwrap_or_default();
                    std::env::set_var("PATH", format!("{};{}", bin_str, current_path));

                    // 2. 🚨 核心：注入 VIPS_MODULE_PATH (指向 lib/，让 libvips 找到 vips-modules-8.x/vips-heif.dll)
                    std::env::set_var("VIPS_MODULE_PATH", &lib_str);

                    std::env::set_var("VIPS_WARNING", "0");
                    println!("🚀 libvips 目录结构注入成功: bin={}, lib={}", bin_str, lib_str);
                } else {
                    eprintln!("⚠️ 致命错误：未找到 resources/vips/bin 或 lib 目录！请检查打包配置。");
                }
            }

            // 🚨 环境变量注入后，再初始化 libvips (此时 PATH 已包含 resources/vips/bin)
            let vips = VipsApp::new("LuminaConvert", false)
                .expect("致命错误: 无法初始化 libvips 引擎，请检查系统是否安装了 vips");
            vips.concurrency_set(num_cpus::get() as i32);
            std::mem::forget(vips); // 防止 drop 导致 libvips shutdown

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            process_images,
            scan_folder,
            cancel_conversion,
            open_output_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
