// Shared worker/UI diagnostics. Never include page text, titles, credentials or full URLs.
(() => {
  const catalog = {
    IMAGE_LOCAL_READ: [
      "所选原图无法读取",
      "读取本地原图",
      "检查所选文件是否存在、是否可读取，然后在队列重新选择。",
    ],
    IMAGE_LOCAL_LOST: [
      "原图选择会话已结束",
      "恢复本地原图",
      "请在队列重新点击选择已下载原图；原任务仍保留。",
    ],
    IMAGE_LOCAL_CANCELLED: [
      "已取消选择，原任务保留",
      "选择本地原图",
      "准备好原图后，可在采集队列再次选择。",
    ],
    BRIDGE_NOT_REGISTERED: [
      "本机连接尚未注册",
      "连接桌面端",
      "启动拾签，在偏好设置中连接 Chrome 扩展或修复连接。",
    ],
    BRIDGE_FORBIDDEN: [
      "Chrome 拒绝本机连接",
      "连接桌面端",
      "核对扩展 ID 和本机连接注册；组织管理的浏览器还需检查策略。",
    ],
    BRIDGE_OFFLINE: [
      "本机连接已中断",
      "连接桌面端",
      "启动拾签后重试；如果仍失败，在偏好设置中修复连接。",
    ],
    BRIDGE_CONFIG: [
      "本机连接配置不可用",
      "读取连接配置",
      "在拾签偏好设置修复 Chrome 连接。",
    ],
    BRIDGE_DISABLED: [
      "浏览器采集已断开",
      "连接桌面端",
      "在拾签偏好设置重新连接 Chrome 扩展。",
    ],
    BRIDGE_AUTH: [
      "本机连接身份校验失败",
      "校验连接",
      "核对扩展 ID，并在拾签偏好设置修复连接。",
    ],
    BRIDGE_BUSY: ["桌面端暂时忙碌", "桌面保存", "等待资料交接完成后重试。"],
    BRIDGE_PROTOCOL: [
      "桌面回执或协议不兼容",
      "校验桌面回执",
      "同时更新桌面端和配套扩展，再重试。",
    ],
    BRIDGE_LIMIT: [
      "本机消息超过限制",
      "传输采集内容",
      "减少采集内容后重新编辑保存。",
    ],
    IMAGE_URL: [
      "图片地址不符合采集要求",
      "检查图片地址",
      "使用公网 HTTP/HTTPS 原图地址；不支持内网、blob、data 或含登录凭据的地址。",
    ],
    IMAGE_DOWNLOAD: [
      "图片下载未完成",
      "下载图片",
      "核对网络及原图地址后重试；下载不携带浏览器登录信息。",
    ],
    IMAGE_ACCESS: [
      "图片服务器拒绝访问",
      "访问图片服务器",
      "在原网页确认图片可访问；需要登录或防盗链的图片可先手动下载再导入。",
    ],
    IMAGE_TYPE: [
      "返回内容不是支持的图片",
      "校验图片格式",
      "确认链接指向 PNG、JPEG、WebP、GIF 或 BMP 图片，而非网页或登录页面。",
    ],
    IMAGE_INVALID: [
      "图片内容损坏或无效",
      "校验图片内容",
      "重新获取原图地址，或手动下载确认后导入。",
    ],
    IMAGE_LIMIT: [
      "图片超过体积或像素限制",
      "校验图片大小",
      "使用不超过 50 MiB、1 亿像素的图片。",
    ],
    CAPTURE_STORAGE: [
      "桌面采集资料写入失败",
      "写入本地资料库",
      "检查磁盘空间和采集目录权限，然后重试。",
    ],
    CAPTURE_TAG_CHANGED: [
      "所选标签已经变化",
      "校验所选标签",
      "在采集队列点击重新编辑，重新选择标签后保存。",
    ],
    CAPTURE_INVALID: [
      "采集信息无效",
      "检查采集内容",
      "根据具体原因修正标题、链接、摘录或标签后重新保存。",
    ],
    CAPTURE_URL: [
      "网页地址无效",
      "检查网页地址",
      "填写完整且不含登录凭据的 HTTP/HTTPS 地址。",
    ],
    CAPTURE_CONFLICT: [
      "采集请求与已有记录冲突",
      "校验采集请求",
      "重新编辑此项后保存；保留诊断信息以便排查。",
    ],
    CAPTURE_REMOVED: [
      "目标资料已被移除",
      "校验目标资料",
      "重新编辑并提交新的采集任务。",
    ],
    CAPTURE_QUEUE_FULL: [
      "浏览器采集队列已满",
      "暂存浏览器队列",
      "先处理或删除旧任务；队列最多保留 200 条。",
    ],
    BROWSER_STORAGE: [
      "浏览器队列无法写入",
      "暂存浏览器队列",
      "保留当前内容，检查浏览器存储空间；不要直接清空扩展数据，以免丢失待处理任务。",
    ],
  };
  function sanitize(value) {
    return String(value || "未提供底层错误")
      .slice(0, 6000)
      .replace(
        /[\x00-\x08\x0b\x0c\x0e-\x1f\x7f\u202a-\u202e\u2066-\u2069]/g,
        "",
      )
      .replace(/https?:\/\/[^\s<>"']+/gi, (value) => {
        try {
          return new URL(value).origin + "/[路径已省略]";
        } catch {
          return "[地址已省略]";
        }
      })
      .replace(/(?:[A-Za-z]:[\\/]|\\\\)[^\r\n,;]*/g, "[本机路径已省略]")
      .replace(/Bearer\s+[^\s,;]+/gi, "Bearer [已隐藏]")
      .replace(
        /((?:api[_-]?key|token|authorization|password|cookie|secret)\s*[:=]\s*)[^\r\n,;]+/gi,
        "$1[已隐藏]",
      )
      .slice(0, 1200);
  }
  const origin = (value) => {
    try {
      return new URL(value).origin;
    } catch {
      return "未提供";
    }
  };
  function diagnose(error, context = {}) {
    const raw = String(error?.message || error || "未收到有效错误原因");
    const code =
      error?.code ||
      raw.match(/^([A-Z][A-Z0-9_]+):/)?.[1] ||
      (/quota|storage.*(?:full|exceed)/i.test(raw)
        ? "BROWSER_STORAGE"
        : "UNKNOWN_ERROR");
    const [summary, stage, baseAdvice] = catalog[code] || [
      "保存未完成，原因待排查",
      context.stage || "提交采集",
      "保留诊断报告，核对内容及连接后重试；未知错误不代表桌面端未运行。",
    ];
    const httpStatus = raw.match(
      /(?:HTTP(?:\/\d(?:\.\d)?)?|status(?: code)?|状态码)\s*[:=]?\s*([45]\d{2})/i,
    )?.[1];
    const httpAdvice = {
      401: "服务器要求身份验证；桌面下载不会携带浏览器登录信息。",
      403: "服务器拒绝访问。可打开来源网页下载原图，再在采集队列选择已下载原图。",
      404: "资源不存在或链接已失效，请重新获取原图地址。",
      408: "服务器请求超时，可稍后重试。",
      413: "服务器认为请求过大，请使用较小图片。",
      429: "服务器限制访问频率，请稍后重试。",
    };
    const retryable =
      !/^(CAPTURE_(URL|INVALID|CONFLICT|REMOVED|TAG_CHANGED|QUEUE_FULL)|IMAGE_(URL|INVALID|TYPE|LIMIT|ACCESS)|BRIDGE_PROTOCOL|BRIDGE_LIMIT)$/.test(
        code,
      );
    return {
      schema: 1,
      code,
      message: summary,
      stage,
      detail: sanitize(raw),
      advice:
        httpAdvice[httpStatus] ||
        (httpStatus >= 500 ? "图片服务器暂时异常，请稍后重试。" : baseAdvice),
      retryable,
      httpStatus: httpStatus || "",
      time: context.time || Date.now(),
      attempts: context.attempts || 0,
      elapsedMs: context.elapsedMs ?? null,
      requestId: context.id || "未生成",
      kind: context.capture?.kind || "未提供",
      source: origin(context.capture?.url),
      image:
        context.capture?.kind === "web_image"
          ? origin(context.capture.imageUrl)
          : "不适用",
      extensionVersion:
        globalThis.chrome?.runtime?.getManifest?.().version || "未知",
      desktopVersion: context.desktopVersion || "尚未获取",
      legacy: !!context.legacy,
    };
  }
  function report(d) {
    return [
      "拾签网页采集诊断",
      `时间：${new Date(d.time).toISOString()}`,
      `错误代码：${d.code}`,
      `失败环节：${d.stage}`,
      `原因：${d.message}`,
      `具体原因：${d.detail}`,
      `处理建议：${d.advice}`,
      `HTTP 状态：${d.httpStatus || "未返回"}`,
      `扩展版本：${d.extensionVersion}`,
      `桌面版本：${d.desktopVersion}`,
      `请求 ID：${d.requestId}`,
      `采集类型：${d.kind}`,
      `来源站点：${d.source}`,
      `图片站点：${d.image}`,
      `尝试次数：${d.attempts}`,
      `本次耗时：${d.elapsedMs ?? "未知"} ms`,
      `自动重试：${d.retryable ? "允许" : "需先处理原因"}`,
      ...(d.legacy ? ["旧记录未保留底层原因，重试后可更新诊断。"] : []),
      "报告已省略正文、标题、完整链接和本机路径；分享前请自行检查剩余内容。",
    ].join("\n");
  }
  function panel(d) {
    const root = document.createElement("section");
    root.className = "failure-diagnostic";
    root.setAttribute("aria-label", "保存失败原因");
    const add = (tag, value) => {
      const el = document.createElement(tag);
      el.textContent = value;
      root.append(el);
      return el;
    };
    add("strong", d.message + (d.httpStatus ? ` · HTTP ${d.httpStatus}` : ""));
    add("p", `${d.stage} · ${d.code}`).className = "hint";
    add("p", d.detail);
    add("p", "建议：" + d.advice);
    const details = add("details", ""),
      summary = document.createElement("summary"),
      pre = document.createElement("pre");
    summary.textContent = "技术详情（已脱敏）";
    pre.textContent = report(d);
    details.append(summary, pre);
    const copy = add("button", "复制诊断报告");
    copy.type = "button";
    copy.className = "secondary";
    copy.onclick = async () => {
      try {
        await navigator.clipboard.writeText(report(d));
        copy.textContent = "已复制";
      } catch {
        copy.textContent = "复制失败，请展开技术详情手动复制";
        details.open = true;
      }
    };
    return root;
  }
  globalThis.ShiqianDiagnostics = { diagnose, sanitize, report, panel };
})();
