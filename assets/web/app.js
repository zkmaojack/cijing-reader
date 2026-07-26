const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => Array.from(document.querySelectorAll(selector));

const PAGE_SIZES = {
  letter: { width: "8.5in", height: "11in" },
  a4: { width: "210mm", height: "297mm" },
  b5: { width: "176mm", height: "250mm" },
  a5: { width: "148mm", height: "210mm" },
};

const PREVIEW_ZOOM_KEY = "cijing-preview-zoom-v1";
const PANE_LAYOUT_KEY = "cijing-pane-layout-v1";
const ARTICLE_HEIGHT_KEY = "cijing-article-height-v1";
const MIN_PANE_WIDTHS = [240, 320, 260];

const state = {
  profiles: [],
  profileByCode: new Map(),
  docxBusy: false,
  pdfBusy: false,
  enhanceBusy: false,
  lexiconBusy: false,
  previewBusy: false,
  previewQueued: false,
  previewTimer: null,
  toastTimer: null,
  previewResizeObserver: null,
  previewZoomMode: "fit",
  previewZoomPercent: 100,
  paneRatios: null,
  paneDrag: null,
  articleResizeActive: false,
  editorTools: null,
};

function showToast(message) {
  const toast = $("#toast");
  toast.textContent = message;
  toast.classList.add("show");
  clearTimeout(state.toastTimer);
  state.toastTimer = setTimeout(() => toast.classList.remove("show"), 2400);
}

function countWords(text) {
  const matches = text.match(/[A-Za-z]+(?:[-'][A-Za-z]+)*/g);
  return matches ? matches.length : 0;
}

function updateWordCount() {
  $("#wordCount").textContent = `${countWords($("#article").value)} words`;
}

function setArticleText(value) {
  if (state.editorTools) {
    state.editorTools.setText(value);
  } else {
    $("#article").value = value;
  }
}

function applyTheme(theme) {
  const normalized = theme === "dark" ? "dark" : "light";
  document.documentElement.dataset.theme = normalized;
  localStorage.setItem("cijing-theme", normalized);
  $("#themeLabel").textContent = normalized === "dark" ? "浅色" : "深色";
}

function toggleTheme() {
  applyTheme(document.documentElement.dataset.theme === "dark" ? "light" : "dark");
}

function parseCustomTags(value) {
  return value
    .split(/[\s,，;；\n]+/)
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => part.split(/[=|:]/)[0].trim())
    .map((part) => part.replace(/^[!*]/, ""))
    .filter(Boolean)
    .slice(0, 18);
}

function updateCustomPreview() {
  const preview = $("#tagPreview");
  preview.innerHTML = "";
  parseCustomTags($("#customWords").value).forEach((tag) => {
    const el = document.createElement("span");
    el.className = "tag";
    el.textContent = tag;
    preview.appendChild(el);
  });
}

function updateGrade() {
  const profile = state.profileByCode.get($("#grade").value);
  if (!profile) return;
  const maxVocab = Math.max(...state.profiles.map((item) => item.estimated_vocab));
  const pct = Math.max(8, Math.round((profile.estimated_vocab / maxVocab) * 100));
  $("#gradeMeter").style.width = `${pct}%`;
  $("#gradeNote").textContent = `预估词汇量约 ${profile.estimated_vocab} 词。${profile.note}`;
}

async function loadProfiles() {
  const response = await fetch("/api/profiles");
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || "年级配置加载失败");
  state.profiles = data.profiles;
  state.profileByCode = new Map(state.profiles.map((profile) => [profile.code, profile]));
  const grade = $("#grade");
  grade.innerHTML = "";
  state.profiles.forEach((profile) => {
    const option = document.createElement("option");
    option.value = profile.code;
    option.textContent = profile.label;
    grade.appendChild(option);
  });
  grade.value = data.default_grade || "P4";
  updateGrade();
}

async function loadDemo() {
  const response = await fetch("/api/demo");
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || "演示文本加载失败");
  state.editorTools?.beforeDestructive("载入演示前");
  state.editorTools?.clearAnnotations();
  $("#title").value = data.title;
  setArticleText(data.text);
  $("#customWords").value = "glittered=ˈɡlɪt.ərd=闪闪发光";
  $("#resultCard").hidden = true;
  updateWordCount();
  updateCustomPreview();
  schedulePreview();
  state.editorTools?.notifyFieldsChanged();
  showToast("已插入演示文本");
}

function clearAll() {
  state.editorTools?.beforeDestructive("清空前");
  $("#title").value = "";
  state.editorTools?.clearAnnotations();
  setArticleText("");
  $("#customWords").value = "";
  $("#resultCard").hidden = true;
  updateWordCount();
  updateCustomPreview();
  setPreviewEmpty("粘贴英文文章后自动生成预览");
  state.editorTools?.notifyFieldsChanged();
  showToast("已清空");
}

function setPdfBusy(value) {
  state.pdfBusy = value;
  $("#pdfBtn").classList.toggle("busy", value);
  $("#pdfLabel").textContent = value ? "下载中..." : "下载 PDF";
}

function setDocxBusy(value) {
  state.docxBusy = value;
  $("#docxBtn").classList.toggle("busy", value);
  $("#docxLabel").textContent = value ? "下载中..." : "下载 DOCX";
}

function setEnhanceBusy(value) {
  state.enhanceBusy = value;
  $("#enhanceBtn").classList.toggle("busy", value);
  $("#enhanceLabel").textContent = value ? "增强中..." : "增强标注";
}

function setLexiconBusy(value) {
  state.lexiconBusy = value;
  $("#lexiconBtn").classList.toggle("busy", value);
  $("#lexiconLabel").textContent = value ? "查询中..." : "查询词库";
}

function readNumber(id) {
  return Number.parseFloat($(id).value);
}

function formatPoint(value) {
  return `${Number.isInteger(value) ? value.toFixed(0) : value.toFixed(1)} pt`;
}

function currentLayoutSettings() {
  const englishPt = readNumber("#englishSize");
  const ipaPt = readNumber("#ipaSize");
  const zhPt = readNumber("#zhSize");
  const pageSize = $("#pageSize").value;
  const customPageWidth = readNumber("#customPageWidth");
  const customPageHeight = readNumber("#customPageHeight");
  return {
    englishPt,
    ipaPt,
    zhPt,
    englishHps: Math.round(englishPt * 2),
    ipaHps: Math.round(ipaPt * 2),
    zhHps: Math.round(zhPt * 2),
    lineHeight: readNumber("#lineHeight"),
    wordSpacing: readNumber("#wordSpacing"),
    pageSize,
    customPageWidth,
    customPageHeight,
  };
}

function applyLayoutSettings() {
  const settings = currentLayoutSettings();
  $("#englishSizeValue").textContent = formatPoint(settings.englishPt);
  $("#ipaSizeValue").textContent = formatPoint(settings.ipaPt);
  $("#zhSizeValue").textContent = formatPoint(settings.zhPt);
  $("#lineHeightValue").textContent = settings.lineHeight.toFixed(2);
  $("#wordSpacingValue").textContent = formatPoint(settings.wordSpacing);

  $("#customPageFields").hidden = settings.pageSize !== "custom";
  const page =
    settings.pageSize === "custom"
      ? {
          width: `${Math.max(90, Math.min(500, settings.customPageWidth || 210))}mm`,
          height: `${Math.max(120, Math.min(700, settings.customPageHeight || 297))}mm`,
        }
      : PAGE_SIZES[settings.pageSize] || PAGE_SIZES.letter;
  const canvas = $("#previewCanvas");
  canvas.style.setProperty("--english-size", `${settings.englishPt}pt`);
  canvas.style.setProperty("--ipa-size", `${settings.ipaPt}pt`);
  canvas.style.setProperty("--zh-size", `${settings.zhPt}pt`);
  canvas.style.setProperty("--line-height", settings.lineHeight.toFixed(2));
  canvas.style.setProperty("--word-spacing", `${settings.wordSpacing}pt`);
  canvas.style.setProperty("--page-width", page.width);
  canvas.style.setProperty("--page-height", page.height);
  requestAnimationFrame(updatePreviewScale);
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function savePreviewZoom() {
  localStorage.setItem(
    PREVIEW_ZOOM_KEY,
    JSON.stringify({
      mode: state.previewZoomMode,
      percent: state.previewZoomPercent,
    }),
  );
}

function restorePreviewZoom() {
  try {
    const saved = JSON.parse(localStorage.getItem(PREVIEW_ZOOM_KEY) || "null");
    if (saved?.mode === "manual") {
      state.previewZoomMode = "manual";
      state.previewZoomPercent = clamp(Number(saved.percent) || 100, 25, 200);
    }
  } catch {
    localStorage.removeItem(PREVIEW_ZOOM_KEY);
  }
}

function updatePreviewScale() {
  const canvas = $("#previewCanvas");
  const page = canvas.querySelector(".preview-page");
  const zoomValue = $("#previewZoomValue");
  const fitButton = $("#previewFitBtn");
  if (!page) {
    zoomValue.textContent =
      state.previewZoomMode === "fit" ? "适合宽度" : `${state.previewZoomPercent}%`;
    fitButton.classList.toggle("active", state.previewZoomMode === "fit");
    return;
  }

  page.style.zoom = "1";
  const canvasStyle = getComputedStyle(canvas);
  const horizontalPadding =
    Number.parseFloat(canvasStyle.paddingLeft) + Number.parseFloat(canvasStyle.paddingRight);
  const availableWidth = Math.max(1, canvas.clientWidth - horizontalPadding - 2);
  const fitScale = clamp(availableWidth / Math.max(1, page.scrollWidth), 0.25, 2);
  const scale =
    state.previewZoomMode === "fit"
      ? fitScale
      : clamp(state.previewZoomPercent / 100, 0.25, 2);
  page.style.zoom = scale.toFixed(3);
  const effectivePercent = Math.round(scale * 100);
  zoomValue.textContent = `${effectivePercent}%`;
  zoomValue.setAttribute("aria-label", `当前预览缩放 ${effectivePercent}%，点击恢复 100%`);
  fitButton.classList.toggle("active", state.previewZoomMode === "fit");
  fitButton.setAttribute("aria-pressed", String(state.previewZoomMode === "fit"));
  $("#previewZoomOutBtn").disabled = effectivePercent <= 25;
  $("#previewZoomInBtn").disabled = effectivePercent >= 200;
}

function setPreviewZoom(percent) {
  state.previewZoomMode = "manual";
  state.previewZoomPercent = clamp(Math.round(percent / 5) * 5, 25, 200);
  savePreviewZoom();
  updatePreviewScale();
}

function stepPreviewZoom(direction) {
  const page = $("#previewCanvas").querySelector(".preview-page");
  const current = page
    ? Math.round((Number.parseFloat(page.style.zoom) || 1) * 100)
    : state.previewZoomPercent;
  setPreviewZoom(current + direction * 10);
}

function fitPreviewWidth() {
  state.previewZoomMode = "fit";
  savePreviewZoom();
  updatePreviewScale();
}

function togglePreviewFocus(force) {
  const root = document.documentElement;
  const active = typeof force === "boolean" ? force : !root.classList.contains("preview-focus");
  root.classList.toggle("preview-focus", active);
  $("#previewFocusBtn").classList.toggle("active", active);
  $("#previewFocusBtn").setAttribute("aria-pressed", String(active));
  $("#previewFocusLabel").textContent = active ? "退出放大" : "放大查看";
  requestAnimationFrame(() => {
    updateResponsivePaneLayout();
    updatePreviewScale();
  });
}

function paneResizeEnabled() {
  const root = document.documentElement;
  return (
    window.innerWidth > 1100 &&
    !root.classList.contains("host-medium") &&
    !root.classList.contains("host-small") &&
    !root.classList.contains("preview-focus")
  );
}

function normalizePaneRatios(ratios) {
  if (!Array.isArray(ratios) || ratios.length !== 3) return null;
  const values = ratios.map(Number);
  const total = values.reduce((sum, value) => sum + value, 0);
  if (values.some((value) => !Number.isFinite(value) || value <= 0) || total <= 0) {
    return null;
  }
  return values.map((value) => value / total);
}

function restorePaneLayout() {
  try {
    state.paneRatios = normalizePaneRatios(
      JSON.parse(localStorage.getItem(PANE_LAYOUT_KEY) || "null"),
    );
  } catch {
    localStorage.removeItem(PANE_LAYOUT_KEY);
  }
}

function currentPaneWidths() {
  return [$(".editor"), $(".preview-panel"), $(".settings")].map(
    (pane) => pane.getBoundingClientRect().width,
  );
}

function constrainedPaneWidths(widths) {
  const total = widths.reduce((sum, value) => sum + value, 0);
  let [left, middle, right] = widths;
  left = clamp(left, MIN_PANE_WIDTHS[0], total - MIN_PANE_WIDTHS[1] - MIN_PANE_WIDTHS[2]);
  right = clamp(right, MIN_PANE_WIDTHS[2], total - left - MIN_PANE_WIDTHS[1]);
  middle = total - left - right;
  if (middle < MIN_PANE_WIDTHS[1]) {
    const missing = MIN_PANE_WIDTHS[1] - middle;
    const leftSpare = Math.max(0, left - MIN_PANE_WIDTHS[0]);
    const takeLeft = Math.min(leftSpare, missing);
    left -= takeLeft;
    right -= missing - takeLeft;
    middle = MIN_PANE_WIDTHS[1];
  }
  return [left, middle, right];
}

function setPaneWidths(widths, persist = false) {
  if (!paneResizeEnabled()) return;
  const workspace = $("#workspace");
  const constrained = constrainedPaneWidths(widths);
  workspace.style.gridTemplateColumns =
    `${constrained[0]}px var(--panel-gap) ${constrained[1]}px var(--panel-gap) ${constrained[2]}px`;
  const total = constrained.reduce((sum, value) => sum + value, 0);
  state.paneRatios = constrained.map((value) => value / total);
  $("#leftPaneResizer").setAttribute(
    "aria-valuenow",
    String(Math.round(state.paneRatios[0] * 100)),
  );
  $("#rightPaneResizer").setAttribute(
    "aria-valuenow",
    String(Math.round((state.paneRatios[0] + state.paneRatios[1]) * 100)),
  );
  if (persist) {
    localStorage.setItem(PANE_LAYOUT_KEY, JSON.stringify(state.paneRatios));
  }
  requestAnimationFrame(updatePreviewScale);
}

function updateResponsivePaneLayout() {
  const workspace = $("#workspace");
  if (!paneResizeEnabled()) {
    workspace.style.gridTemplateColumns = "";
    return;
  }
  if (!state.paneRatios) return;
  const gutters =
    $("#leftPaneResizer").getBoundingClientRect().width +
    $("#rightPaneResizer").getBoundingClientRect().width;
  const available = Math.max(
    MIN_PANE_WIDTHS.reduce((sum, value) => sum + value, 0),
    workspace.clientWidth - gutters,
  );
  setPaneWidths(state.paneRatios.map((ratio) => ratio * available));
}

function startPaneDrag(event, index) {
  if (!paneResizeEnabled() || event.button !== 0) return;
  const handle = event.currentTarget;
  state.paneDrag = {
    index,
    pointerId: event.pointerId,
    startX: event.clientX,
    widths: currentPaneWidths(),
  };
  handle.setPointerCapture(event.pointerId);
  handle.classList.add("dragging");
  document.body.classList.add("is-resizing-layout");
  event.preventDefault();
}

function movePaneDrag(event) {
  const drag = state.paneDrag;
  if (!drag || drag.pointerId !== event.pointerId) return;
  const delta = event.clientX - drag.startX;
  const widths = [...drag.widths];
  if (drag.index === 0) {
    const pairTotal = widths[0] + widths[1];
    widths[0] = clamp(
      widths[0] + delta,
      MIN_PANE_WIDTHS[0],
      pairTotal - MIN_PANE_WIDTHS[1],
    );
    widths[1] = pairTotal - widths[0];
  } else {
    const pairTotal = widths[1] + widths[2];
    widths[1] = clamp(
      widths[1] + delta,
      MIN_PANE_WIDTHS[1],
      pairTotal - MIN_PANE_WIDTHS[2],
    );
    widths[2] = pairTotal - widths[1];
  }
  setPaneWidths(widths);
}

function finishPaneDrag(event) {
  if (!state.paneDrag || state.paneDrag.pointerId !== event.pointerId) return;
  event.currentTarget.classList.remove("dragging");
  document.body.classList.remove("is-resizing-layout");
  state.paneDrag = null;
  setPaneWidths(currentPaneWidths(), true);
}

function resizePanesWithKeyboard(event, index) {
  if (!paneResizeEnabled() || !["ArrowLeft", "ArrowRight"].includes(event.key)) return;
  event.preventDefault();
  const widths = currentPaneWidths();
  const delta = (event.key === "ArrowLeft" ? -1 : 1) * (event.shiftKey ? 32 : 12);
  if (index === 0) {
    widths[0] += delta;
    widths[1] -= delta;
  } else {
    widths[1] += delta;
    widths[2] -= delta;
  }
  setPaneWidths(widths, true);
}

function resetPaneLayout() {
  state.paneRatios = null;
  localStorage.removeItem(PANE_LAYOUT_KEY);
  $("#workspace").style.gridTemplateColumns = "";
  requestAnimationFrame(updatePreviewScale);
  showToast("已恢复默认栏宽");
}

function restoreArticleHeight() {
  const height = Number.parseFloat(localStorage.getItem(ARTICLE_HEIGHT_KEY));
  if (Number.isFinite(height) && height >= 160) {
    $(".article-editor-wrap").style.height = `${height}px`;
  }
}

function resetArticleHeight() {
  localStorage.removeItem(ARTICLE_HEIGHT_KEY);
  $(".article-editor-wrap").style.height = "";
  state.editorTools?.syncHighlights?.();
  showToast("已恢复输入框高度");
}

function requestPayload(article) {
  return {
    article,
    title: $("#title").value,
    grade: $("#grade").value,
    customWords: $("#customWords").value,
    annotateUnknown: $("#annotateUnknown").checked,
    ...currentLayoutSettings(),
  };
}

function downloadGeneratedFile(downloadUrl, filename) {
  if (window.__CIJING_DESKTOP__) return;
  const link = document.createElement("a");
  link.href = downloadUrl;
  link.download = filename;
  link.rel = "noopener";
  document.body.appendChild(link);
  link.click();
  link.remove();
}

function setPreviewEmpty(message) {
  clearTimeout(state.previewTimer);
  $("#previewStatus").textContent = "等待文章";
  $("#previewCanvas").innerHTML = `<div class="preview-empty-state">${message}</div>`;
  applyLayoutSettings();
}

function setPreviewHtml(html, missingCount) {
  $("#previewCanvas").innerHTML = html;
  $("#previewStatus").textContent = missingCount ? `${missingCount} 个未收录词` : "预览已生成";
  applyLayoutSettings();
  state.editorTools?.applyPreviewAnnotations();
}

function schedulePreview() {
  clearTimeout(state.previewTimer);
  const article = $("#article").value.trim();
  if (!article) {
    setPreviewEmpty("粘贴英文文章后自动生成预览");
    return;
  }
  $("#previewStatus").textContent = "更新中...";
  state.previewTimer = setTimeout(refreshPreview, 320);
}

async function refreshPreview() {
  if (state.previewBusy) {
    state.previewQueued = true;
    return;
  }
  const article = $("#article").value.trim();
  if (!article) {
    setPreviewEmpty("粘贴英文文章后自动生成预览");
    return;
  }
  state.previewBusy = true;
  try {
    const response = await fetch("/api/preview", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(requestPayload(article)),
    });
    const data = await response.json();
    if (!response.ok || !data.ok) throw new Error(data.error || "预览生成失败");
    setPreviewHtml(data.html, data.missingCount || 0);
  } catch (error) {
    $("#previewStatus").textContent = "预览失败";
    showToast(error.message || "预览生成失败");
  } finally {
    state.previewBusy = false;
    if (state.previewQueued) {
      state.previewQueued = false;
      schedulePreview();
    }
  }
}

async function generateFile(format) {
  const isPdf = format === "pdf";
  if ((isPdf && state.pdfBusy) || (!isPdf && state.docxBusy)) return;
  const article = $("#article").value.trim();
  if (!article) {
    showToast("请先粘贴英文文章");
    $("#article").focus();
    return;
  }

  const label = isPdf ? "PDF" : "DOCX";
  const setBusy = isPdf ? setPdfBusy : setDocxBusy;
  setBusy(true);
  $("#resultCard").hidden = true;
  try {
    const response = await fetch(`/api/generate-${format}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(requestPayload(article)),
    });
    const data = await response.json();
    if (!response.ok || !data.ok) throw new Error(data.error || `${label} 生成失败`);

    downloadGeneratedFile(data.downloadUrl, data.filename);
    $("#resultTitle").textContent = `${label} 已生成`;
    const destination = window.__CIJING_DESKTOP__
      ? `${data.filename} 已保存到软件目录的 output 文件夹，点击此处打开。`
      : `已开始下载 ${data.filename}。`;
    $("#resultMeta").textContent = data.missingCount
      ? `${destination} 有 ${data.missingCount} 个未收录词保持原文。`
      : destination;
    $("#resultCard").hidden = false;
    const menu = $(".output-menu");
    if (menu) menu.open = false;
    showToast(
      window.__CIJING_DESKTOP__ ? `${label} 已生成并保存` : `${label} 已生成，已开始下载`,
    );
  } catch (error) {
    showToast(error.message || `${label} 输出失败`);
  } finally {
    setBusy(false);
  }
}

async function enhanceAnnotations() {
  if (state.enhanceBusy) return;
  const article = $("#article").value.trim();
  if (!article) {
    showToast("请先粘贴英文文章");
    $("#article").focus();
    return;
  }
  const endpoint = $("#aiEndpoint").value.trim();
  const model = $("#aiModel").value.trim();
  if (!endpoint || !model) {
    showToast("请填写接口地址和模型");
    return;
  }

  setEnhanceBusy(true);
  try {
    const response = await fetch("/api/ai-enhance", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        article,
        title: $("#title").value,
        grade: $("#grade").value,
        endpoint,
        model,
        apiKey: $("#aiKey").value,
      }),
    });
    const data = await response.json();
    if (!response.ok || !data.ok) throw new Error(data.error || "AI 增强失败");
    const current = $("#customWords").value.trim();
    $("#customWords").value = current ? `${current}\n${data.annotations}` : data.annotations;
    updateCustomPreview();
    schedulePreview();
    state.editorTools?.notifyFieldsChanged();
    showToast("AI 标注已追加，请检查后输出");
  } catch (error) {
    showToast(error.message || "AI 增强失败");
  } finally {
    setEnhanceBusy(false);
  }
}

async function enhanceFromNetworkLexicon() {
  if (state.lexiconBusy) return;
  const article = $("#article").value.trim();
  if (!article) {
    showToast("请先粘贴英文文章");
    $("#article").focus();
    return;
  }
  const endpoint = $("#lexiconEndpoint").value.trim();
  if (!endpoint) {
    showToast("请填写网络词库接口");
    $("#lexiconEndpoint").focus();
    return;
  }

  setLexiconBusy(true);
  try {
    const response = await fetch("/api/network-lexicon", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        article,
        grade: $("#grade").value,
        customWords: $("#customWords").value,
        endpoint,
        apiKey: $("#lexiconKey").value,
      }),
    });
    const data = await response.json();
    if (!response.ok || !data.ok) throw new Error(data.error || "网络词库查询失败");
    const current = $("#customWords").value.trim();
    $("#customWords").value = current ? `${current}\n${data.annotations}` : data.annotations;
    updateCustomPreview();
    schedulePreview();
    state.editorTools?.notifyFieldsChanged();
    showToast(`网络词库已补全 ${data.count || 0} 个词`);
  } catch (error) {
    showToast(error.message || "网络词库查询失败");
  } finally {
    setLexiconBusy(false);
  }
}

function wireEvents() {
  $("#themeBtn").addEventListener("click", toggleTheme);
  $("#demoBtn").addEventListener("click", loadDemo);
  $("#clearBtn").addEventListener("click", clearAll);
  $("#docxBtn").addEventListener("click", () => generateFile("docx"));
  $("#pdfBtn").addEventListener("click", () => generateFile("pdf"));
  $("#enhanceBtn").addEventListener("click", enhanceAnnotations);
  $("#lexiconBtn").addEventListener("click", enhanceFromNetworkLexicon);
  $("#resultCard").addEventListener("click", () => {
    if (window.__CIJING_DESKTOP__ && window.ipc) {
      window.ipc.postMessage("open-output-folder");
    }
  });
  $("#article").addEventListener("input", () => {
    updateWordCount();
    schedulePreview();
  });
  $("#previewZoomOutBtn").addEventListener("click", () => stepPreviewZoom(-1));
  $("#previewZoomInBtn").addEventListener("click", () => stepPreviewZoom(1));
  $("#previewZoomValue").addEventListener("click", () => setPreviewZoom(100));
  $("#previewFitBtn").addEventListener("click", fitPreviewWidth);
  $("#previewFocusBtn").addEventListener("click", () => togglePreviewFocus());
  $("#resetArticleHeightBtn").addEventListener("click", resetArticleHeight);

  const articleWrap = $(".article-editor-wrap");
  articleWrap.addEventListener(
    "pointerdown",
    (event) => {
      const rect = articleWrap.getBoundingClientRect();
      state.articleResizeActive =
        event.clientX >= rect.right - 28 && event.clientY >= rect.bottom - 28;
    },
    true,
  );
  window.addEventListener("pointerup", () => {
    if (!state.articleResizeActive) return;
    state.articleResizeActive = false;
    localStorage.setItem(
      ARTICLE_HEIGHT_KEY,
      String(Math.round(articleWrap.getBoundingClientRect().height)),
    );
  });

  [
    ["#leftPaneResizer", 0],
    ["#rightPaneResizer", 1],
  ].forEach(([selector, index]) => {
    const handle = $(selector);
    handle.addEventListener("pointerdown", (event) => startPaneDrag(event, index));
    handle.addEventListener("pointermove", movePaneDrag);
    handle.addEventListener("pointerup", finishPaneDrag);
    handle.addEventListener("pointercancel", finishPaneDrag);
    handle.addEventListener("keydown", (event) => resizePanesWithKeyboard(event, index));
    handle.addEventListener("dblclick", resetPaneLayout);
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && document.documentElement.classList.contains("preview-focus")) {
      togglePreviewFocus(false);
    }
  });
  $("#title").addEventListener("input", schedulePreview);
  $("#grade").addEventListener("change", () => {
    updateGrade();
    schedulePreview();
  });
  $("#customWords").addEventListener("input", () => {
    updateCustomPreview();
    schedulePreview();
  });
  $("#annotateUnknown").addEventListener("change", schedulePreview);
  window.addEventListener("resize", () => {
    updateResponsivePaneLayout();
    updatePreviewScale();
  });
  window.visualViewport?.addEventListener("resize", updatePreviewScale);
  if ("ResizeObserver" in window) {
    state.previewResizeObserver = new ResizeObserver(updatePreviewScale);
    state.previewResizeObserver.observe($("#previewCanvas"));
  }
  $$(".range-field input, #pageSize, #customPageWidth, #customPageHeight").forEach((control) => {
    control.addEventListener("input", applyLayoutSettings);
    control.addEventListener("change", applyLayoutSettings);
  });
}

async function boot() {
  applyTheme(localStorage.getItem("cijing-theme") || "light");
  restorePreviewZoom();
  restorePaneLayout();
  restoreArticleHeight();
  wireEvents();
  if (!window.CijingEditorTools) {
    throw new Error("编辑工具加载失败");
  }
  state.editorTools = window.CijingEditorTools.init({
    showToast,
    onRestore() {
      updateWordCount();
      updateCustomPreview();
      updateGrade();
      applyLayoutSettings();
      schedulePreview();
    },
  });
  applyLayoutSettings();
  requestAnimationFrame(updateResponsivePaneLayout);
  await loadProfiles();
  const restored = state.editorTools.restoreDraft();
  updateWordCount();
  updateCustomPreview();
  if (restored) {
    updateGrade();
    applyLayoutSettings();
  }
  schedulePreview();
}

boot().catch((error) => showToast(error.message || "页面初始化失败"));
