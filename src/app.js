// DeepSeek Harness 设置页逻辑（无框架，直接使用 __TAURI__ core invoke）
(function () {
  'use strict';

  const $ = (id) => document.getElementById(id);

  // withGlobalTauri 注入的 IPC（在普通浏览器中打开时给出提示而不是静默失败）
  const invoke = window.__TAURI__ && window.__TAURI__.core
    ? window.__TAURI__.core.invoke.bind(window.__TAURI__.core)
    : null;
  if (!invoke) {
    document.addEventListener('DOMContentLoaded', () => {
      msg('error', '此页面需要在 DeepSeek Harness 桌面应用内运行');
    });
    return;
  }

  const urlInput = $('url');
  const titleInput = $('title');
  const appNameInput = $('appName');
  const widthInput = $('width');
  const heightInput = $('height');
  const immersiveInput = $('immersive');
  const bgTransparentInput = $('bgTransparent');
  const showTrayInput = $('showTray');
  const opacityInput = $('opacity');
  const opacityVal = $('opacityVal');
  const iconInputs = Array.from(document.querySelectorAll('input[name="iconMode"]'));
  const previewDefault = $('previewDefault');
  const previewImg = $('previewImg');
  const iconNote = $('iconNote');
  const dropZone = $('dropZone');
  const fileInput = $('fileInput');
  const dropName = $('dropName');
  const msgEl = $('msg');
  const saveBtn = $('saveBtn');
  const launchBtn = $('launchBtn');
  const firstRun = $('firstRun');

  const state = {
    // 上传模式：本会话选择的本地图片 dataURL
    uploadDataUrl: null,
    // 探测缓存：url -> dataURL（避免重复请求）
    autoCache: new Map(),
    lastProbedUrl: '',
    storedIconDataUrl: null, // 已保存配置中的图标
    busy: false,
  };

  const preview = {
    showDefault(note) {
      previewDefault.hidden = false;
      previewImg.hidden = true;
      if (note) iconNote.textContent = note;
    },
    showImage(dataUrl, note) {
      previewImg.src = dataUrl;
      previewDefault.hidden = true;
      previewImg.hidden = false;
      if (note) iconNote.textContent = note;
    },
  };

  function msg(kind, text) {
    msgEl.hidden = !text;
    msgEl.className = 'msg' + (kind ? ' ' + kind : '');
    msgEl.textContent = text || '';
  }

  function currentMode() {
    const el = iconInputs.find((i) => i.checked);
    return el ? el.value : 'default';
  }

  function setBusy(b) {
    state.busy = b;
    saveBtn.disabled = b;
    launchBtn.disabled = b;
  }

  /** 客户端规范化网址，返回 {url, origin} 或抛错 */
  function normalizeUrl(raw) {
    let s = String(raw || '').trim();
    if (!s) throw new Error('请填写要启动的网页地址');
    if (!/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(s)) s = 'https://' + s;
    let u;
    try {
      u = new URL(s);
    } catch (e) {
      throw new Error('网址格式无效');
    }
    if (u.protocol !== 'http:' && u.protocol !== 'https:') {
      throw new Error('仅支持 http / https 网页地址');
    }
    return { url: u.href.replace(/\/$/, '') || u.href, origin: u.origin };
  }

  function updateDefaultNote() {
    iconNote.textContent = state.uploadDataUrl
      ? '已选择本地图片（未保存）'
      : '内置默认图标';
  }

  function renderMode() {
    const mode = currentMode();
    dropZone.hidden = mode !== 'upload';
    previewDefault.hidden = true;
    previewImg.hidden = true;

    if (mode === 'default') {
      if (state.storedIconDataUrl) {
        preview.showImage(state.storedIconDataUrl, '内置默认图标');
      } else {
        preview.showDefault('内置默认图标');
      }
    } else if (mode === 'auto') {
      if (state.lastProbedUrl && state.autoCache.get(state.lastProbedUrl)) {
        preview.showImage(state.autoCache.get(state.lastProbedUrl), '自动获取网站图标');
      } else if (state.storedIconDataUrl) {
        preview.showImage(state.storedIconDataUrl, '已保存的网站图标');
      } else {
        preview.showDefault('保存时自动探测网站图标');
      }
      maybeProbe();
    } else {
      // upload
      if (state.uploadDataUrl) {
        preview.showImage(state.uploadDataUrl, '本地图片（未保存）');
      } else if (state.storedIconDataUrl) {
        preview.showImage(state.storedIconDataUrl, '已保存的本地图片');
      } else {
        preview.showDefault('尚未选择图片');
      }
    }
  }

  /** 自动模式：防抖探测 favicon 用于预览 */
  function maybeProbe() {
    if (currentMode() !== 'auto' || state.busy) return;
    clearTimeout(maybeProbe._t);
    maybeProbe._t = setTimeout(async () => {
      let norm;
      try {
        norm = normalizeUrl(urlInput.value);
      } catch (e) {
        return; // 网址未完成时静默
      }
      const key = norm.url;
      if (state.autoCache.has(key) && state.lastProbedUrl === key) return;
      state.lastProbedUrl = key;
      iconNote.textContent = '正在探测网站图标…';
      try {
        const res = await invoke('probe_favicon', { url: norm.url });
        if (currentMode() !== 'auto') return;
        if (res.ok && res.dataUrl) {
          state.autoCache.set(key, res.dataUrl);
          preview.showImage(res.dataUrl, '自动获取网站图标');
        } else {
          state.autoCache.set(key, '');
          preview.showDefault('未能探测到图标，将使用默认图标');
        }
      } catch (err) {
        preview.showDefault('探测失败，将使用默认图标');
      }
    }, 600);
  }

  function onFile(file) {
    if (!file) return;
    const ok = /^image\/(png|jpeg|x-icon|vnd.microsoft.icon)$/.test(file.type) ||
      /\.(png|jpe?g|ico)$/i.test(file.name);
    if (!ok) {
      msg('error', '仅支持 PNG / JPG / ICO 图片');
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      state.uploadDataUrl = String(reader.result);
      dropName.textContent = file.name;
      preview.showImage(state.uploadDataUrl, '本地图片（未保存）');
      msg('');
      fileInput.value = ''; // 允许再次选择同一文件时仍触发 change
    };
    reader.onerror = () => msg('error', '读取图片失败');
    reader.readAsDataURL(file);
  }

  function payload() {
    const norm = normalizeUrl(urlInput.value);
    const mode = currentMode();
    let w = parseInt(widthInput.value, 10);
    let h = parseInt(heightInput.value, 10);
    w = Number.isFinite(w) ? Math.min(7680, Math.max(640, w)) : 1280;
    h = Number.isFinite(h) ? Math.min(4320, Math.max(480, h)) : 860;
    return {
      url: norm.url,
      title: titleInput.value.trim(),
      appName: appNameInput.value.trim(),
      icon: mode,
      uploadIcon: mode === 'upload' ? state.uploadDataUrl || null : null,
      width: w,
      height: h,
      immersive: immersiveInput.checked,
      bgTransparent: bgTransparentInput.checked,
      showTray: showTrayInput.checked,
      opacity: parseInt(opacityInput.value, 10) || 100,
    };
  }

  async function save(launch) {
    if (state.busy) return;
    msg('');
    let input;
    try {
      input = payload();
    } catch (e) {
      msg('error', e.message);
      return;
    }
    setBusy(true);
    launchBtn.textContent = '启动中…';
    try {
      const res = await invoke('save_config', { input, launch });
      if (res.error) {
        msg('error', res.error);
        return;
      }
      state.storedIconDataUrl = res.iconDataUrl || state.storedIconDataUrl;
      state.uploadDataUrl = null; // 已入库，预览交给已保存状态
      if (res.iconDataUrl) {
        preview.showImage(res.iconDataUrl, currentMode() === 'default' ? '内置默认图标' : '图标已更新');
      }
      const warn = (res.warnings || []).map((w) => '· ' + w).join('\n');
      msg(launch ? 'ok' : 'ok',
        launch ? '已保存，正在重新启动…' + (warn ? '\n' + warn : '')
               : '已保存 ✔' + (warn ? '\n' + warn : ''));
      if (!launch) {
        firstRun.hidden = true;
      }
      // launch 时后端会关闭本设置窗口；窗口消失后这里不会再执行
    } catch (err) {
      msg('error', String(err && err.message ? err.message : err));
    } finally {
      setBusy(false);
      launchBtn.textContent = '保存并重新启动';
    }
  }

  async function init() {
    try {
      const st = await invoke('get_setup_state');
      if (st && st.hasConfig && st.config) {
        const c = st.config;
        urlInput.value = c.url || '';
        titleInput.value = c.title || '';
        appNameInput.value = c.appName || '';
        widthInput.value = c.width || 1280;
        heightInput.value = c.height || 860;
        immersiveInput.checked = !!c.immersive;
        bgTransparentInput.checked = !!c.bgTransparent;
        showTrayInput.checked = c.showTray !== false;
        opacityInput.value = c.opacity || 100;
        opacityVal.textContent = (c.opacity || 100) + '%';
        const mode = ['default', 'auto', 'upload'].includes(c.icon) ? c.icon : 'default';
        iconInputs.find((i) => i.value === mode).checked = true;
        state.storedIconDataUrl = st.iconDataUrl || null;
        if (mode === 'upload' && state.storedIconDataUrl) {
          preview.showImage(state.storedIconDataUrl, '已保存的本地图片');
        } else if (mode === 'auto' && state.storedIconDataUrl) {
          state.autoCache.set((c.url || '').replace(/\/$/, ''), state.storedIconDataUrl);
        }
        firstRun.hidden = true;
      } else {
        firstRun.hidden = false;
        iconInputs.find((i) => i.value === 'default').checked = true;
      }
    } catch (err) {
      console.error(err);
    }
    renderMode();
  }

  // ---- 事件 ----
  opacityInput.addEventListener('input', () => {
    opacityVal.textContent = opacityInput.value + '%';
    clearTimeout(opacityInput._t);
    opacityInput._t = setTimeout(() => {
      const v = parseInt(opacityInput.value, 10) || 100;
      invoke('live_set_opacity', { opacity: v }).catch(() => {});
    }, 80);
  });
  urlInput.addEventListener('input', () => {
    state.lastProbedUrl = '';
    if (currentMode() === 'auto') maybeProbe();
  });
  iconInputs.forEach((i) => i.addEventListener('change', renderMode));

  // 点击“使用本地图片”：选中该模式并打开文件选择框。
  // 不依赖浏览器默认 label→input 行为；.click() 必须在用户手势回调内同步执行，
  // 否则 WebKitGTK 可能判定为非用户激活而拒绝弹出原生选择框。
  $('uploadModeLabel').addEventListener('click', (e) => {
    e.preventDefault();
    const radio = iconInputs.find((i) => i.value === 'upload');
    if (!radio.checked) {
      radio.checked = true;
      radio.dispatchEvent(new Event('change', { bubbles: true }));
    }
    fileInput.click();
  });
  fileInput.addEventListener('change', () => onFile(fileInput.files[0]));
  dropZone.addEventListener('click', () => fileInput.click());
  ['dragover', 'dragenter'].forEach((ev) =>
    dropZone.addEventListener(ev, (e) => { e.preventDefault(); dropZone.classList.add('drag'); }));
  ['dragleave', 'drop'].forEach((ev) =>
    dropZone.addEventListener(ev, (e) => { e.preventDefault(); dropZone.classList.remove('drag'); }));
  dropZone.addEventListener('drop', (e) => onFile(e.dataTransfer.files[0]));

  saveBtn.addEventListener('click', () => save(false));
  launchBtn.addEventListener('click', () => save(true));

  // 浏览器原生窗口可被直接关闭（设置窗口带系统标题栏）
  document.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') save(true);
  });

  init();
})();
