<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { open } from '@tauri-apps/plugin-dialog';
  import { onMount, onDestroy } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { LazyStore } from '@tauri-apps/plugin-store';
  import { sendNotification } from '@tauri-apps/plugin-notification';

  const store = new LazyStore('settings.dat');

  interface FileItem { id: string; path: string; }
  let files: FileItem[] = $state([]);
  let selectedIds: Set<string> = $state(new Set());
  let isProcessing = $state(false);
  let statusMessage = $state("等待添加任务...");
  let unlisten: (() => void) | null = null;

  let outputDir = $state("");
  let targetFormat = $state("jpeg");
  let preset = $state("standard");
  let metadataPolicy = $state("strip");
  let resizeMode = $state("none");
  let resizeValue = $state(1920);
  let bitDepth = $state("auto");
  let renameTemplate = $state("{original}");

  let progress = $state({ completed: 0, total: 0, filename: '', status: '' });
  let unlistenProgress: (() => void) | null = null;
  let unlistenFinished: (() => void) | null = null;

  let showSettings = $state(false);
  let perfMode = $state('beast');
  let customConcurrency = $state(0);

  function generateId() { return Date.now().toString(36) + Math.random().toString(36).substr(2); }

  async function saveSetting(key: string, value: any) {
    await store.set(key, value);
    await store.save();
  }

  onMount(async () => {
    unlisten = await getCurrentWindow().onDragDropEvent((event) => {
      if (event.payload.type === 'drop') {
        addPaths(event.payload.paths);
      }
    });

    unlistenProgress = await listen<any>('convert-progress', (event) => {
      progress = event.payload;
    });

    unlistenFinished = await listen<any>('conversion-finished', (event) => {
      sendNotification({
        title: 'LuminaConvert 转换完成',
        body: `成功 ${event.payload.success} 张，失败 ${event.payload.failed} 张`
      });
    });

    perfMode = (await store.get('perfMode')) || 'beast';
    customConcurrency = (await store.get('customConcurrency')) || 0;
    outputDir = (await store.get('outputDir')) || '';
    renameTemplate = (await store.get('renameTemplate')) || '{original}';
  });

  onDestroy(() => {
    if (unlisten) unlisten();
    if (unlistenProgress) unlistenProgress();
    if (unlistenFinished) unlistenFinished();
  });

  async function addPaths(paths: string[]) {
    const imagePaths = paths.filter(path =>
      /\.(jpg|jpeg|png|heic|heif|hif|webp|avif|jxl|tiff|tif|bmp|gif|svg|pdf)$/i.test(path)
    );
    const newItems = imagePaths
      .filter(p => !files.some(f => f.path === p))
      .map(p => ({ id: generateId(), path: p }));
    if (newItems.length > 0) {
      files = [...files, ...newItems];
      statusMessage = `已追加 ${newItems.length} 个新文件 (已自动去重)`;
    }
  }

  async function addFiles() {
    const selected = await open({
      multiple: true,
      filters: [{ name: 'Images', extensions: ['jpg', 'jpeg', 'png', 'heic', 'heif', 'hif', 'webp', 'avif', 'jxl', 'tiff', 'tif', 'bmp', 'gif', 'svg', 'pdf'] }]
    });
    if (selected) addPaths(Array.isArray(selected) ? selected : [selected]);
  }

  async function addFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      statusMessage = "正在扫描文件夹...";
      try {
        const scannedFiles = await invoke<string[]>('scan_folder', { path: selected });
        const newItems = scannedFiles
          .filter(p => !files.some(f => f.path === p))
          .map(p => ({ id: generateId(), path: p }));
        files = [...files, ...newItems];
        statusMessage = `扫描完成，共发现 ${scannedFiles.length} 张图片 (已去重)`;
      } catch (e) {
        statusMessage = `扫描失败: ${e}`;
      }
    }
  }

  async function selectOutputDir() {
    const selected = await open({ directory: true, multiple: false });
    if (selected) outputDir = selected;
  }

  function toggleSelectAll() {
    if (selectedIds.size === files.length) {
      selectedIds = new Set();
    } else {
      selectedIds = new Set(files.map(f => f.id));
    }
  }

  function toggleSelect(id: string) {
    if (selectedIds.has(id)) {
      const next = new Set(selectedIds);
      next.delete(id);
      selectedIds = next;
    } else {
      selectedIds = new Set([...selectedIds, id]);
    }
  }

  function removeSelected() {
    files = files.filter(f => !selectedIds.has(f.id));
    selectedIds = new Set();
  }

  async function startConversion() {
    if (files.length === 0) return;
    if (!outputDir) {
      statusMessage = "错误: 请先选择导出文件夹！";
      return;
    }

    isProcessing = true;
    progress = { completed: 0, total: files.length, filename: '', status: '' };
    statusMessage = "引擎启动中... (硬件加速已就绪)";

    try {
      await saveSetting('outputDir', outputDir);
      await saveSetting('renameTemplate', renameTemplate);
      await saveSetting('perfMode', perfMode);
      await saveSetting('customConcurrency', customConcurrency);

      const result = await invoke<string>('process_images', {
        paths: files.map(f => f.path),
        options: {
          format: targetFormat,
          preset: preset,
          metadata: metadataPolicy,
          output_dir: outputDir,
          resize_mode: resizeMode,
          resize_value: resizeValue,
          bit_depth: bitDepth,
          rename_template: renameTemplate,
          concurrency: customConcurrency,
          perf_mode: perfMode
        }
      });
      statusMessage = result;
      if (!result.includes('取消')) {
        files = [];
        selectedIds = new Set();
      }
    } catch (e) {
      statusMessage = `发生错误: ${e}`;
    } finally {
      isProcessing = false;
    }
  }

  async function cancelTask() {
    await invoke('cancel_conversion');
    statusMessage = "⚠️ 正在安全终止当前任务 (底层 C 库无法瞬间中断，请稍候)...";
  }

  async function openOutput() {
    if (outputDir) {
      await invoke('open_output_dir', { path: outputDir });
    }
  }
</script>

<svelte:window ondragover={(e) => e.preventDefault()} ondrop={(e) => e.preventDefault()} />

<main class="app-container">
  <header>
    <h1>LuminaConvert <span class="badge">Safe Core</span></h1>
    <div class="actions">
      <button class="btn-secondary" onclick={addFiles}>+ 文件</button>
      <button class="btn-secondary" onclick={addFolder}>+ 文件夹</button>
      <button class="btn-icon" onclick={() => showSettings = true} title="高级设置">⚙️</button>
    </div>
  </header>

  {#if showSettings}
    <div class="modal-overlay" onclick={() => showSettings = false}>
      <div class="modal-content" onclick={(e) => e.stopPropagation()}>
        <h2>⚙️ 算力与偏好设置</h2>
        
        <div class="setting-group">
          <label>性能调度模式</label>
          <div class="segmented-control">
            <button class:active={perfMode === 'beast'} onclick={() => { perfMode = 'beast'; saveSetting('perfMode', 'beast'); }}>
              🚀 极速 (满载)
            </button>
            <button class:active={perfMode === 'balanced'} onclick={() => { perfMode = 'balanced'; saveSetting('perfMode', 'balanced'); }}>
              ⚖️ 平衡 (50%)
            </button>
            <button class:active={perfMode === 'background'} onclick={() => { perfMode = 'background'; saveSetting('perfMode', 'background'); }}>
              🐢 后台 (不卡顿)
            </button>
          </div>
          <p class="hint">
            {#if perfMode === 'beast'}
              榨干所有 CPU 核心与 libvips 底层线程，适合挂机渲染。
            {:else if perfMode === 'balanced'}
              占用一半算力，保证你可以流畅浏览网页或看视频。
            {:else}
              强制 libvips 单线程运行，彻底把电脑让给游戏或其他重度软件。
            {/if}
          </p>
        </div>

        <div class="setting-group">
          <label>自定义并发数 (0 为自动根据模式计算)</label>
          <input type="number" min="0" max="64" bind:value={customConcurrency} class="num-input" onchange={() => saveSetting('customConcurrency', customConcurrency)} />
        </div>

        <div class="modal-footer">
          <button class="btn-primary" onclick={() => showSettings = false}>保存并关闭</button>
        </div>
      </div>
    </div>
  {/if}

  <section class="control-panel">
    <div class="control-group full-width">
      <label>目标格式 (次世代与专业级)</label>
      <div class="format-grid">
        <button class:active={targetFormat === 'jpeg'} onclick={() => targetFormat = 'jpeg'}>JPEG</button>
        <button class:active={targetFormat === 'png'} onclick={() => targetFormat = 'png'}>PNG</button>
        <button class:active={targetFormat === 'webp'} onclick={() => targetFormat = 'webp'}>WebP</button>
        <button class:active={targetFormat === 'avif'} onclick={() => targetFormat = 'avif'} title="次世代高压缩比">AVIF</button>
        <button class:active={targetFormat === 'jxl'} onclick={() => targetFormat = 'jxl'} title="JPEG XL 未来标准">JXL</button>
        <button class:active={targetFormat === 'tiff'} onclick={() => targetFormat = 'tiff'} title="专业 16-bit/印刷">TIFF</button>
        <button class:active={targetFormat === 'bmp'} onclick={() => targetFormat = 'bmp'}>BMP</button>
        <button class:active={targetFormat === 'gif'} onclick={() => targetFormat = 'gif'}>GIF</button>
      </div>
    </div>

    <div class="control-group">
      <label>压缩档位</label>
      <div class="preset-grid">
        <button class:active={preset === 'fast'} onclick={() => preset = 'fast'}>
          <span class="preset-title">极速</span><span class="preset-desc">小体积</span>
        </button>
        <button class:active={preset === 'standard'} onclick={() => preset = 'standard'}>
          <span class="preset-title">标准</span><span class="preset-desc">平衡</span>
        </button>
        <button class:active={preset === 'high'} onclick={() => preset = 'high'}>
          <span class="preset-title">高质量</span><span class="preset-desc">高画质</span>
        </button>
        <button class:active={preset === 'lossless'} onclick={() => preset = 'lossless'}>
          <span class="preset-title">无损</span><span class="preset-desc">原画质</span>
        </button>
      </div>
    </div>

    <div class="control-group">
      <label>元数据 (EXIF/隐私)</label>
      <div class="segmented-control vertical">
        <button class:active={metadataPolicy === 'strip'} onclick={() => metadataPolicy = 'strip'} title="抹除GPS/相机信息，体积最小">🔥 极限剥离</button>
        <button class:active={metadataPolicy === 'icc'} onclick={() => metadataPolicy = 'icc'} title="剥离隐私，保留广色域ICC配置(推荐)">🎨 保留色彩</button>
        <button class:active={metadataPolicy === 'all'} onclick={() => metadataPolicy = 'all'} title="原封不动保留所有数据">📷 完整保留</button>
      </div>
    </div>

    <div class="control-group output-dir">
      <label>导出至</label>
      <div class="dir-picker">
        <span title={outputDir}>{outputDir || '未选择...'}</span>
        <button class="btn-small" onclick={selectOutputDir}>浏览</button>
      </div>
    </div>
  </section>

  <section class="advanced-panel">
    <div class="control-group">
      <label>尺寸控制</label>
      <div class="segmented-control">
        <button class:active={resizeMode === 'none'} onclick={() => resizeMode = 'none'}>原尺寸</button>
        <button class:active={resizeMode === 'fit'} onclick={() => resizeMode = 'fit'}>限制最大边</button>
        <button class:active={resizeMode === 'percent'} onclick={() => resizeMode = 'percent'}>按比例缩放</button>
      </div>
      {#if resizeMode !== 'none'}
        <input type="number" bind:value={resizeValue} class="num-input"
               placeholder={resizeMode === 'fit' ? '如: 1920' : '如: 50'} />
      {/if}
    </div>

    <div class="control-group">
      <label>色彩与位深</label>
      <div class="segmented-control">
        <button class:active={bitDepth === 'auto'} onclick={() => bitDepth = 'auto'}>自动 (HDR映射)</button>
        <button class:active={bitDepth === '16bit'} onclick={() => bitDepth = '16bit'}>强制 16-bit</button>
        <button class:active={bitDepth === '8bit'} onclick={() => bitDepth = '8bit'}>强制 8-bit</button>
      </div>
    </div>

    <div class="control-group full-width">
      <label>重命名规则 <span class="hint">变量: {'{original}'}, {'{width}'}, {'{height}'}, {'{counter}'}</span></label>
      <input type="text" bind:value={renameTemplate} class="text-input" placeholder={'例: web_{width}x{height}_{counter}'} />
    </div>
  </section>

  <section class="file-list-container">
    <div class="list-header">
      <label class="checkbox-wrapper">
        <input type="checkbox" checked={selectedIds.size === files.length && files.length > 0} onchange={toggleSelectAll} />
        <span>全选 ({files.length})</span>
      </label>
      <button class="btn-danger" onclick={removeSelected} disabled={selectedIds.size === 0}>移除选中</button>
    </div>

    <ul class="file-list">
      {#each files.slice(0, 100) as item}
        <li class:selected={selectedIds.has(item.id)}>
          <label class="checkbox-wrapper">
            <input type="checkbox" checked={selectedIds.has(item.id)} onchange={(e) => {
              const newSet = new Set(selectedIds);
              if (e.currentTarget.checked) {
                newSet.add(item.id);
              } else {
                newSet.delete(item.id);
              }
              selectedIds = newSet;
            }} />
            <span title={item.path}>{item.path.split(/[/\\]/).pop()}</span>
          </label>
        </li>
      {/each}
      
      {#if files.length > 100}
        <li class="more-hint">
          ... 及其他 {files.length - 100} 个文件 (已在后台安全队列中)
        </li>
      {/if}
    </ul>
  </section>

  <footer>
    {#if isProcessing && progress.total > 0}
      <div class="progress-container">
        <div class="progress-info">
          <span class="progress-text">{progress.completed} / {progress.total}</span>
          <span class="progress-filename" title={progress.filename}>
            {statusMessage.includes('终止') ? '⏳ 正在终止...' : progress.filename}
          </span>
        </div>
        <div class="progress-bar-bg">
          <div class="progress-bar-fill" style="width: {(progress.completed / progress.total) * 100}%"></div>
        </div>
      </div>
    {/if}

    <div class="footer-actions">
      {#if isProcessing}
        <button class="btn-danger btn-large" onclick={cancelTask}>⏹ 强制取消</button>
      {:else}
        <button class="btn-primary" onclick={() => startConversion()} disabled={files.length === 0 || !outputDir}>
          开始批量转换
        </button>
        {#if statusMessage.includes('完成') || statusMessage.includes('终止')}
          <button class="btn-secondary" onclick={openOutput}>📂 打开输出目录</button>
        {/if}
      {/if}
    </div>

    <div class="status-bar">状态: {statusMessage} {#if !isProcessing}(files:{files.length}, output:{!!outputDir}){/if}</div>
  </footer>
</main>

<style>
  :global(body) { margin: 0; font-family: system-ui, -apple-system, sans-serif; background: #121212; color: #e0e0e0; overflow: hidden; user-select: none; }
  .app-container { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; height: 100vh; box-sizing: border-box; }
  header { display: flex; justify-content: space-between; align-items: center; flex-shrink: 0; }
  h1 { margin: 0; font-size: 1.2rem; display: flex; align-items: center; gap: 8px; }
  .badge { font-size: 0.6rem; background: #10b981; padding: 2px 6px; border-radius: 4px; color: white; }
  .actions { display: flex; gap: 10px; }

  .control-panel { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 15px; background: #1e1e1e; padding: 15px; border-radius: 8px; border: 1px solid #333; flex-shrink: 0; }
  .control-group { display: flex; flex-direction: column; gap: 5px; font-size: 0.85rem; color: #aaa; }
  .full-width { grid-column: 1 / -1; }
  .dir-picker { display: flex; gap: 10px; align-items: center; }
  .dir-picker span { flex: 1; background: #2a2a2a; padding: 6px; border-radius: 4px; border: 1px solid #444; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-size: 0.8rem; }

  .format-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; }
  .format-grid button { background: #222; border: 1px solid #333; border-radius: 6px; padding: 8px; font-size: 0.85rem; color: #ccc; transition: all 0.2s; cursor: pointer; }
  .format-grid button.active { border-color: #10b981; background: #064e3b; color: #6ee7b7; font-weight: 600; }

  .preset-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; }
  .preset-grid button { background: #222; border: 1px solid #333; border-radius: 6px; padding: 8px 4px; display: flex; flex-direction: column; align-items: center; gap: 2px; transition: all 0.2s; cursor: pointer; }
  .preset-grid button.active { border-color: #3b82f6; background: #1e3a8a; }
  .preset-title { font-size: 0.8rem; font-weight: 600; color: #eee; }
  .preset-desc { font-size: 0.65rem; color: #888; }
  .preset-grid button.active .preset-desc { color: #93c5fd; }

  .segmented-control { display: flex; background: #222; border-radius: 6px; padding: 2px; border: 1px solid #333; }
  .segmented-control button { flex: 1; background: transparent; border: none; color: #888; padding: 6px 0; font-size: 0.8rem; border-radius: 4px; transition: all 0.2s; cursor: pointer; }
  .segmented-control button.active { background: #3b82f6; color: white; font-weight: 600; box-shadow: 0 2px 4px rgba(0,0,0,0.2); }
  .segmented-control.vertical { flex-direction: column; gap: 4px; }
  .segmented-control.vertical button { text-align: left; padding: 8px 12px; font-size: 0.8rem; }

  .advanced-panel { display: grid; grid-template-columns: 1fr 1fr; gap: 15px; background: #1a1a1a; padding: 15px; border-radius: 8px; border: 1px solid #2a2a2a; flex-shrink: 0; }
  .num-input, .text-input { width: 100%; background: #222; border: 1px solid #444; color: #eee; padding: 8px; border-radius: 4px; margin-top: 6px; font-family: monospace; box-sizing: border-box; }
  .num-input:focus, .text-input:focus { outline: none; border-color: #3b82f6; }
  .hint { font-size: 0.7rem; color: #666; font-weight: normal; }
  .file-list-container { flex: 1; display: flex; flex-direction: column; background: #181818; border: 1px solid #333; border-radius: 8px; overflow: hidden; min-height: 0; }
  .list-header { display: flex; justify-content: space-between; align-items: center; padding: 10px 15px; background: #222; border-bottom: 1px solid #333; font-size: 0.9rem; flex-shrink: 0; }
  .file-list { flex: 1; overflow-y: auto; margin: 0; padding: 0; list-style: none; }
  .file-list li { padding: 8px 15px; border-bottom: 1px solid #222; font-size: 0.85rem; font-family: monospace; transition: background 0.1s; }
  .file-list li.selected { background: #2d3748; }
  .file-list li:hover { background: #252525; }
  .more-hint { text-align: center; color: #3b82f6; font-weight: 600; background: #1e293b; border: 1px dashed #3b82f6; margin: 10px; border-radius: 6px; padding: 12px; }
  .checkbox-wrapper { display: flex; align-items: center; gap: 10px; cursor: pointer; width: 100%; }
  .checkbox-wrapper span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  footer { display: flex; flex-direction: column; gap: 10px; flex-shrink: 0; }

  .progress-container { background: #181818; border: 1px solid #333; border-radius: 8px; padding: 12px; }
  .progress-info { display: flex; justify-content: space-between; margin-bottom: 8px; font-size: 0.85rem; }
  .progress-text { color: #3b82f6; font-weight: 700; font-family: monospace; }
  .progress-filename { color: #888; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 70%; text-align: right; }
  .progress-bar-bg { height: 6px; background: #2a2a2a; border-radius: 3px; overflow: hidden; }
  .progress-bar-fill { height: 100%; background: linear-gradient(90deg, #3b82f6, #60a5fa); transition: width 0.1s linear; box-shadow: 0 0 10px #3b82f6; }

  .footer-actions { display: flex; gap: 15px; }

  .btn-icon { background: transparent; border: 1px solid #444; color: #aaa; width: 36px; height: 36px; border-radius: 50%; font-size: 1.2rem; display: flex; align-items: center; justify-content: center; transition: all 0.2s; }
  .btn-icon:hover { background: #333; color: #fff; border-color: #666; }

  .modal-overlay { position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; background: rgba(0,0,0,0.6); backdrop-filter: blur(4px); display: flex; align-items: center; justify-content: center; z-index: 100; }
  .modal-content { background: #1e1e1e; border: 1px solid #333; border-radius: 12px; padding: 24px; width: 450px; max-width: 90%; box-shadow: 0 10px 25px rgba(0,0,0,0.5); }
  .modal-content h2 { margin-top: 0; font-size: 1.2rem; border-bottom: 1px solid #333; padding-bottom: 12px; margin-bottom: 20px; }
  .setting-group { margin-bottom: 20px; }
  .setting-group label { display: block; font-size: 0.9rem; color: #ccc; margin-bottom: 8px; font-weight: 600; }
  .hint { font-size: 0.75rem; color: #888; margin-top: 8px; line-height: 1.4; }
  .modal-footer { text-align: right; margin-top: 24px; border-top: 1px solid #333; padding-top: 16px; }

  button { cursor: pointer; border: none; border-radius: 4px; font-weight: 500; transition: opacity 0.2s; }
  button:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-secondary { background: #333; color: #ddd; padding: 8px 12px; font-size: 0.85rem; }
  .btn-secondary:hover:not(:disabled) { background: #444; }
  .btn-small { background: #444; color: #ddd; padding: 6px 10px; font-size: 0.8rem; }
  .btn-danger { background: #ef4444; color: white; padding: 6px 12px; font-size: 0.8rem; }
  .btn-danger.btn-large { flex: 1; padding: 14px; font-size: 1.1rem; }
  .btn-primary { background: #3b82f6; color: white; padding: 12px; font-size: 1rem; flex: 1; box-shadow: 0 4px 6px rgba(0,0,0,0.3); }
  .btn-primary:hover:not(:disabled) { background: #2563eb; }
  .btn-primary:disabled { opacity: 0.5; }
  .status-bar { font-family: monospace; font-size: 0.8rem; color: #10b981; background: #000; padding: 8px; border-radius: 4px; text-align: center; border: 1px solid #222; }
</style>
