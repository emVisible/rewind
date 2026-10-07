// 浏览器适配层:让桌面壳(Tauri)与 Web 版共用同一套 UI 代码,不维护两份界面。
// 仅当运行在浏览器(无 Tauri 注入)时生效。
(function () {
  if (window.__TAURI__) return;

  const listeners = {};
  let jobId = null;
  let since = 0;
  let timer = null;

  async function post(cmd, args) {
    const r = await fetch('/api', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ cmd, args: args || {} }),
    });
    const v = await r.json();
    if (v && v.error) throw new Error(v.error);
    return v;
  }

  function dispatch(env) {
    const type = env && env.type;
    if (!type) return;
    const ch = 'rewind://' + type;
    const payload = Object.assign({}, env);
    delete payload.type;
    (listeners[ch] || []).forEach((cb) => {
      try { cb({ payload }); } catch (e) { /* 单回调异常不影响轮询 */ }
    });
  }

  function poll() {
    if (timer) clearInterval(timer);
    timer = setInterval(async () => {
      if (jobId === null) return;
      try {
        const v = await post('events', { job: jobId, since });
        (v.events || []).forEach(dispatch);
        if (typeof v.next === 'number') since = v.next;
        if (v.done) { jobId = null; since = 0; }
      } catch (e) { /* 服务断开时静默重试 */ }
    }, 350);
  }

  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        if (cmd === 'start_jobs' || cmd === 'reclip_job') {
          const v = await post(cmd, args);
          jobId = v.job;
          since = 0;
          poll();
          return v;
        }
        if (cmd === 'cancel_jobs') return post('cancel_jobs', args);
        if (cmd === 'get_settings') return post('get_settings', args);
        if (cmd === 'set_settings') return post('set_settings', args);
        if (cmd === 'app_info') return post('app_info', args);
        if (cmd === 'use_era') return (await post('use_era', args)).preset;
        if (cmd === 'reroll_preset') return (await post('reroll_preset', args)).seeds;
        if (cmd === 'save_recipe') return post('save_recipe', args);
        if (cmd === 'preview_compare') return post('preview_compare', args);
        if (cmd === 'sample_frames') return post('sample_frames', args);
        // Web 版的成品/预览帧都走 /file 白名单,服务端登记过,前端无需再授权
        if (cmd === 'view_result') return args.path;
        if (cmd === 'probe_file') return post('probe_file', args);
        if (cmd === 'sample_file') return post('sample_file', args);
        throw new Error('Web 版暂未开放: ' + cmd);
      },
      convertFileSrc: (p) => '/file' + encodeURIComponent(p),
    },
    event: {
      listen: async (name, cb) => {
        (listeners[name] = listeners[name] || []).push(cb);
        return () => {};
      },
    },
  };

  // 浏览器给不出磁盘绝对路径:把 File 原样 POST 给本地服务,换回绝对路径。
  window.__REWIND_WEB__ = true;
  window.__REWIND_UPLOAD__ = async (file) => {
    const r = await fetch('/upload?name=' + encodeURIComponent(file.name), {
      method: 'POST',
      headers: { 'Content-Type': 'application/octet-stream' },
      body: file,
    });
    const v = await r.json().catch(() => ({}));
    if (!r.ok || !v.path) throw new Error(v.error || `上传失败(HTTP ${r.status})`);
    return v.path;
  };
})();
