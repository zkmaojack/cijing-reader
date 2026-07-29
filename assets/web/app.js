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
const BUILTIN_TRANSLATION_DELAY = 1200;
const TRANSLATION_FAILURE_RETRY_DELAY = 60 * 1000;
const TRANSLATION_FALLBACK_RETRY_DELAY = 5 * 60 * 1000;
const AUTO_TRANSLATION_CACHE_VERSION = 2;
const MAX_AUTO_TRANSLATION_CACHE_ENTRIES = 12;

function t(key, fallback, variables) {
  const translated = window.YujieI18n?.t?.(key, variables);
  return translated && translated !== key ? translated : fallback;
}

const state = {
  profiles: [],
  profileByCode: new Map(),
  docxBusy: false,
  pdfBusy: false,
  translateBusy: false,
  translateTimer: null,
  translateRequestId: 0,
  translateAbortController: null,
  translationAutoRetryAfter: new Map(),
  activeTranslationKey: null,
  previewBusy: false,
  previewQueued: false,
  previewTimer: null,
  previewState: "waiting",
  previewMissingCount: 0,
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
  toast.textContent = window.YujieI18n?.t?.(String(message)) || message;
  toast.classList.add("show");
  clearTimeout(state.toastTimer);
  state.toastTimer = setTimeout(() => toast.classList.remove("show"), 2400);
}

function countWords(text) {
  const matches = text.match(/[A-Za-z]+(?:[-'][A-Za-z]+)*/g);
  return matches ? matches.length : 0;
}

function updateWordCount() {
  const count = countWords($("#article").value);
  $("#wordCount").textContent = t("editor.wordCount", `${count} words`, { count });
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
  $("#themeLabel").textContent =
    normalized === "dark"
      ? t("action.light", "浅色")
      : t("action.dark", "深色");
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
  const note = t(`grade.${profile.code}.note`, profile.note);
  $("#gradeNote").textContent = t(
    "grade.summary",
    `预估词汇量约 ${profile.estimated_vocab} 词。${note}`,
    { count: profile.estimated_vocab, note },
  );
}

function updateGradeOptions() {
  const grade = $("#grade");
  Array.from(grade.options).forEach((option) => {
    const profile = state.profileByCode.get(option.value);
    if (!profile) return;
    option.textContent = t(`grade.${profile.code}.label`, profile.label);
  });
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
    option.textContent = t(`grade.${profile.code}.label`, profile.label);
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
  saveActiveAutoTranslations();
  cancelBuiltinTranslationRequest();
  state.editorTools?.clearAnnotations();
  $("#title").value = data.title;
  setArticleText(data.text);
  $("#customWords").value =
    $("#translationLanguage").value === "zh-Hans"
      ? "glittered=ˈɡlɪt.ərd=闪闪发光"
      : "glittered";
  activateCurrentTranslationContext();
  $("#resultCard").hidden = true;
  updateWordCount();
  updateCustomPreview();
  schedulePreview();
  scheduleBuiltinTranslation();
  state.editorTools?.notifyFieldsChanged();
  showToast("已插入演示文本");
}

function clearAll() {
  state.editorTools?.beforeDestructive("清空前");
  $("#title").value = "";
  state.editorTools?.clearAnnotations();
  setArticleText("");
  $("#customWords").value = "";
  resetAutoTranslationCache();
  $("#resultCard").hidden = true;
  updateWordCount();
  updateCustomPreview();
  setPreviewEmpty();
  state.editorTools?.notifyFieldsChanged();
  showToast("已清空");
}

function setPdfBusy(value) {
  state.pdfBusy = value;
  $("#pdfBtn").classList.toggle("busy", value);
  $("#pdfLabel").textContent = value
    ? t("download.inProgress", "下载中...")
    : t("download.pdf", "下载 PDF");
}

function setDocxBusy(value) {
  state.docxBusy = value;
  $("#docxBtn").classList.toggle("busy", value);
  $("#docxLabel").textContent = value
    ? t("download.inProgress", "下载中...")
    : t("download.docx", "下载 DOCX");
}

function setTranslateBusy(value) {
  state.translateBusy = value;
  $("#translateBtn").classList.toggle("busy", value);
  $("#translateBtn").disabled = value || $("#translationLanguage").value === "zh-Hans";
  $("#translateLabel").textContent = value
    ? t("translation.translating", "翻译中...")
    : t("translation.button", "更新多语释义");
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
      state.previewZoomMode === "fit"
        ? t("preview.fit", "适合宽度")
        : `${state.previewZoomPercent}%`;
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
  zoomValue.setAttribute(
    "aria-label",
    t(
      "preview.currentZoom",
      `当前预览缩放 ${effectivePercent}%，点击恢复 100%`,
      { percent: effectivePercent },
    ),
  );
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
  $("#previewFocusLabel").textContent = active
    ? t("preview.exitFocus", "退出放大")
    : t("preview.focus", "放大查看");
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
  const direction = document.documentElement.dir === "rtl" ? -1 : 1;
  const delta = (event.clientX - drag.startX) * direction;
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
  const direction = document.documentElement.dir === "rtl" ? -1 : 1;
  const delta =
    (event.key === "ArrowLeft" ? -1 : 1) *
    direction *
    (event.shiftKey ? 32 : 12);
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

function normalizeTranslationSignatureText(value) {
  return String(value || "").replace(/\r\n?/g, "\n").trim();
}

function translationSignatureHash(value) {
  const text = String(value);
  let first = 2166136261;
  let second = 5381;
  for (let index = 0; index < text.length; index += 1) {
    const code = text.charCodeAt(index);
    first ^= code;
    first = Math.imul(first, 16777619);
    second = Math.imul(second, 33) ^ code;
  }
  return `${(first >>> 0).toString(36)}-${(second >>> 0).toString(36)}`;
}

function currentTranslationContext() {
  const article = normalizeTranslationSignatureText($("#article").value);
  const customWords = normalizeTranslationSignatureText($("#customWords").value);
  const grade = $("#grade").value || "P4";
  const targetLanguage = $("#translationLanguage").value || "zh-Hans";
  const source = `${targetLanguage}\u0000${grade}\u0000${article}\u0000${customWords}`;
  return {
    article,
    customWords,
    grade,
    targetLanguage,
    key: [
      "v1",
      targetLanguage,
      grade,
      article.length,
      customWords.length,
      translationSignatureHash(source),
    ].join(":"),
  };
}

function readAutoTranslationCache() {
  const raw = $("#autoTranslationCache").value.trim();
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (
      !Array.isArray(parsed) &&
      parsed?.version !== AUTO_TRANSLATION_CACHE_VERSION
    ) {
      return [];
    }
    const entries = Array.isArray(parsed) ? parsed : parsed?.entries;
    if (!Array.isArray(entries)) return [];
    return entries
      .filter(
        (entry) =>
          entry &&
          typeof entry.key === "string" &&
          typeof entry.annotations === "string" &&
          entry.annotations.length <= 512 * 1024,
      )
      .map((entry) => ({
        key: entry.key,
        annotations: entry.annotations,
        usedAt: Number.isFinite(entry.usedAt) ? entry.usedAt : 0,
      }))
      .sort((left, right) => right.usedAt - left.usedAt)
      .slice(0, MAX_AUTO_TRANSLATION_CACHE_ENTRIES);
  } catch (_error) {
    return [];
  }
}

function writeAutoTranslationCache(entries, { notify = true } = {}) {
  const compact = entries
    .filter((entry) => entry?.key && typeof entry.annotations === "string")
    .sort((left, right) => right.usedAt - left.usedAt)
    .slice(0, MAX_AUTO_TRANSLATION_CACHE_ENTRIES);
  $("#autoTranslationCache").value = JSON.stringify({
    version: AUTO_TRANSLATION_CACHE_VERSION,
    entries: compact,
  });
  if (notify) state.editorTools?.notifyFieldsChanged();
}

function cacheAutoTranslations(key, annotations) {
  const value = String(annotations || "").trim();
  if (!key || !value) return;
  const entries = readAutoTranslationCache().filter((entry) => entry.key !== key);
  entries.unshift({ key, annotations: value, usedAt: Date.now() });
  writeAutoTranslationCache(entries);
}

function saveActiveAutoTranslations() {
  cacheAutoTranslations(state.activeTranslationKey, $("#autoTranslations").value);
}

function findCachedAutoTranslations(key) {
  const entries = readAutoTranslationCache();
  const found = entries.find((entry) => entry.key === key);
  if (!found) return "";
  found.usedAt = Date.now();
  writeAutoTranslationCache(entries, { notify: false });
  return found.annotations;
}

function combinedCustomWords() {
  return [$("#autoTranslations").value, $("#customWords").value]
    .map((value) => String(value || "").trim())
    .filter(Boolean)
    .join("\n");
}

function requestPayload(article) {
  return {
    article,
    title: $("#title").value,
    grade: $("#grade").value,
    targetLanguage: $("#translationLanguage").value,
    pronunciationScheme: $("#pronunciationScheme").value,
    customWords: combinedCustomWords(),
    annotateUnknown: $("#annotateUnknown").checked,
    ...currentLayoutSettings(),
  };
}

function selectedLanguageLabel() {
  const option = $("#translationLanguage").selectedOptions[0];
  return option ? option.textContent.split("·")[0].trim() : "中文（简体）";
}

function updateLanguageUi({ notify = false } = {}) {
  const language = $("#translationLanguage").value;
  const pronunciation = $("#pronunciationScheme").value;
  const languageLabel = selectedLanguageLabel();
  $("#definitionSizeLabel").textContent = t(
    "settings.definitionLanguageSize",
    `${languageLabel}释义大小`,
    { language: languageLabel },
  );
  const pronunciationNote =
    pronunciation === "ipa-us"
      ? t("pronunciation.usOffline", "美式 IPA 由内置词典生成")
      : pronunciation === "none"
        ? t("pronunciation.disabled", "已关闭注音")
        : t("pronunciation.builtin", "所选注音方式由内置发音词典生成");
  $("#languageNote").textContent =
    language === "zh-Hans"
      ? t("translation.chineseNote", `简体中文释义使用内置词典；${pronunciationNote}。`, {
          pronunciation: pronunciationNote,
        })
      : t(
          "translation.networkNote",
          `${languageLabel}首次生成需要联网，结果会随草稿缓存；${pronunciationNote}。`,
          { language: languageLabel, pronunciation: pronunciationNote },
        );
  $("#translateBtn").disabled = state.translateBusy || language === "zh-Hans";
  if (notify) {
    showToast(
      language === "zh-Hans"
        ? t("translation.switchedChinese", "已切换至简体中文内置词典")
        : t("translation.switched", `已切换至${languageLabel}，即将自动翻译`, {
            language: languageLabel,
          }),
    );
  }
}

function selectedInterfaceLanguageName(locale = window.YujieI18n?.getLocale?.()) {
  const select = $("#interfaceLanguage");
  const option = Array.from(select.options).find((item) => item.value === locale);
  return option?.textContent?.trim() || locale || "";
}

function setInterfaceLanguageStatus(key, fallback, locale) {
  const status = $("#interfaceLanguageStatus");
  if (!status) return;
  status.removeAttribute("title");
  const language = selectedInterfaceLanguageName(locale);
  status.textContent = t(key, fallback.replace("{language}", language), { language });
}

function refreshInterfaceLanguageStatus({ failed = false, locale } = {}) {
  const i18n = window.YujieI18n;
  const activeLocale = i18n?.getLocale?.() || "zh-Hans";
  const displayLocale = locale || activeLocale;
  const language = selectedInterfaceLanguageName(displayLocale);
  if (failed) {
    setInterfaceLanguageStatus(
      "uiLanguage.failed",
      `此版本未包含${language}界面，已继续使用当前界面。`,
      displayLocale,
    );
  } else if (i18n?.isBuiltInLocale?.(activeLocale)) {
    setInterfaceLanguageStatus(
      "uiLanguage.base",
      "全部可选界面语言均已内置，可离线即时切换。",
      locale,
    );
  } else if (!i18n?.isLocaleLoaded?.(activeLocale)) {
    setInterfaceLanguageStatus(
      "uiLanguage.failed",
      `此版本未包含${language}界面，已继续使用当前界面。`,
      displayLocale,
    );
  } else {
    setInterfaceLanguageStatus(
      "uiLanguage.ready",
      `${language}界面已内置并就绪。`,
      activeLocale,
    );
  }
}

function refreshLocaleDependentUi() {
  updateGradeOptions();
  updateGrade();
  updateWordCount();
  applyTheme(document.documentElement.dataset.theme || "light");
  updateLanguageUi();
  setTranslateBusy(state.translateBusy);
  setDocxBusy(state.docxBusy);
  setPdfBusy(state.pdfBusy);
  refreshPreviewLocaleStatus();
  scheduleBuiltinTranslation();
  state.editorTools?.refreshLocale?.();
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

function setPreviewEmpty() {
  clearTimeout(state.previewTimer);
  state.previewState = "waiting";
  state.previewMissingCount = 0;
  $("#previewStatus").textContent = t("preview.waiting", "等待文章");
  const emptyState = document.createElement("div");
  emptyState.className = "preview-empty-state";
  emptyState.textContent = t("preview.empty", "粘贴英文文章后自动生成预览");
  $("#previewCanvas").replaceChildren(emptyState);
  applyLayoutSettings();
}

function setPreviewHtml(html, missingCount) {
  state.previewState = "ready";
  state.previewMissingCount = missingCount;
  $("#previewCanvas").innerHTML = html;
  $("#previewStatus").textContent = missingCount
    ? t("preview.missing", `${missingCount} 个未收录词`, { count: missingCount })
    : t("preview.ready", "预览已生成");
  applyLayoutSettings();
  state.editorTools?.applyPreviewAnnotations();
}

function refreshPreviewLocaleStatus() {
  if (state.previewState === "waiting") {
    $("#previewStatus").textContent = t("preview.waiting", "等待文章");
    const emptyState = $("#previewCanvas").querySelector(".preview-empty-state");
    if (emptyState) {
      emptyState.textContent = t("preview.empty", "粘贴英文文章后自动生成预览");
    }
  } else if (state.previewState === "updating") {
    $("#previewStatus").textContent = t("preview.updating", "更新中...");
  } else if (state.previewState === "failed") {
    $("#previewStatus").textContent = t("preview.failed", "预览失败");
  } else {
    $("#previewStatus").textContent = state.previewMissingCount
      ? t("preview.missing", `${state.previewMissingCount} 个未收录词`, {
          count: state.previewMissingCount,
        })
      : t("preview.ready", "预览已生成");
  }
  updatePreviewScale();
}

function schedulePreview() {
  clearTimeout(state.previewTimer);
  const article = $("#article").value.trim();
  if (!article) {
    setPreviewEmpty();
    return;
  }
  state.previewState = "updating";
  $("#previewStatus").textContent = t("preview.updating", "更新中...");
  state.previewTimer = setTimeout(refreshPreview, 320);
}

async function refreshPreview() {
  if (state.previewBusy) {
    state.previewQueued = true;
    return;
  }
  const article = $("#article").value.trim();
  if (!article) {
    setPreviewEmpty();
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
    state.previewState = "failed";
    $("#previewStatus").textContent = t("preview.failed", "预览失败");
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
    if (!response.ok || !data.ok) {
      throw new Error(
        data.error ||
          t("output.generateFailed", `${label} 生成失败`, {
            format: label,
          }),
      );
    }

    downloadGeneratedFile(data.downloadUrl, data.filename);
    $("#resultTitle").textContent = t("output.generatedTitle", `${label} 已生成`, {
      format: label,
    });
    const destination = window.__CIJING_DESKTOP__
      ? t(
          "output.savedDesktop",
          `${data.filename} 已保存到软件目录的 output 文件夹，点击此处打开。`,
          { filename: data.filename },
        )
      : t("output.downloadStarted", `已开始下载 ${data.filename}。`, {
          filename: data.filename,
        });
    $("#resultMeta").textContent = data.missingCount
      ? t(
          "output.missingWords",
          `${destination} 有 ${data.missingCount} 个未收录词保持原文。`,
          { destination, count: data.missingCount },
        )
      : destination;
    $("#resultCard").hidden = false;
    const menu = $(".output-menu");
    if (menu) menu.open = false;
    showToast(
      window.__CIJING_DESKTOP__
        ? t("output.generatedSaved", `${label} 已生成并保存`, { format: label })
        : t("output.generatedDownloaded", `${label} 已生成，已开始下载`, {
            format: label,
          }),
    );
  } catch (error) {
    showToast(
      error.message ||
        t("output.failed", `${label} 输出失败`, {
          format: label,
        }),
    );
  } finally {
    setBusy(false);
  }
}

function cancelBuiltinTranslationRequest() {
  clearTimeout(state.translateTimer);
  state.translateTimer = null;
  state.translateRequestId += 1;
  if (state.translateAbortController) {
    state.translateAbortController.abort();
    state.translateAbortController = null;
  }
  if (state.translateBusy) setTranslateBusy(false);
}

function setActiveAutoTranslations(value) {
  const normalized = String(value || "");
  if ($("#autoTranslations").value === normalized) return;
  $("#autoTranslations").value = normalized;
  state.editorTools?.notifyFieldsChanged();
}

function looksLikeChineseFallback(annotations, targetLanguage) {
  if (
    !annotations ||
    ["zh-Hans", "zh-Hant", "ja"].includes(targetLanguage)
  ) {
    return false;
  }
  const definitions = String(annotations)
    .split(/\r?\n/)
    .map((line) => line.split("=").slice(2).join("=").trim())
    .filter(Boolean);
  if (!definitions.length) return false;
  const chineseDefinitions = definitions.filter((definition) =>
    /[\u3400-\u9fff]/u.test(definition),
  ).length;
  return chineseDefinitions / definitions.length >= 0.5;
}

function activateCurrentTranslationContext({ migrateLegacy = false } = {}) {
  let legacyAnnotations = $("#autoTranslations").value.trim();
  if (
    migrateLegacy &&
    looksLikeChineseFallback(
      legacyAnnotations,
      $("#translationLanguage").value || "zh-Hans",
    )
  ) {
    $("#autoTranslations").value = "";
    $("#autoTranslationCache").value = "";
    legacyAnnotations = "";
  }
  if (migrateLegacy && !state.activeTranslationKey && legacyAnnotations) {
    const legacyContext = currentTranslationContext();
    if (legacyContext.targetLanguage !== "zh-Hans" && legacyContext.article) {
      cacheAutoTranslations(legacyContext.key, legacyAnnotations);
    }
  } else {
    saveActiveAutoTranslations();
  }

  cancelBuiltinTranslationRequest();
  const context = currentTranslationContext();
  state.activeTranslationKey = context.key;
  const cached =
    context.targetLanguage !== "zh-Hans" && context.article
      ? findCachedAutoTranslations(context.key)
      : "";
  setActiveAutoTranslations(cached);
  return { context, cached };
}

function resetAutoTranslationCache() {
  cancelBuiltinTranslationRequest();
  state.translationAutoRetryAfter.clear();
  state.activeTranslationKey = currentTranslationContext().key;
  setActiveAutoTranslations("");
  $("#autoTranslationCache").value = "";
  state.editorTools?.notifyFieldsChanged();
}

function setTranslationStatus(message) {
  $("#translationStatus").textContent = window.YujieI18n?.t?.(String(message)) || message;
}

function localTranslationFallbackStatus(reason = "unavailable", preservedExisting = false) {
  const locale = window.YujieI18n?.getLocale?.() || "zh-Hans";
  if (locale === "zh-Hans") {
    if (preservedExisting) {
      return "在线更新暂不可用，已保留现有目标语释义；可稍后点击更新重试。";
    }
    return reason === "busy"
      ? "正在完成上一条翻译请求，将自动重试…"
      : "在线目标语翻译暂不可用，未使用中文回退；稍后将自动重试，也可点击更新重试。";
  }
  if (locale === "en") {
    if (preservedExisting) {
      return "Online update is unavailable. Existing target-language definitions were kept; click Update later to retry.";
    }
    return reason === "busy"
      ? "Finishing the previous translation request; retrying automatically…"
      : "Online target-language translation is unavailable. No Chinese fallback was used; retry will run later, or click Update.";
  }
  return `${t("translation.failed", "Built-in translation failed")} · ${t(
    "translation.queued",
    "Translation will retry later",
  )}`;
}

function blockAutomaticTranslationRetry(key, delay) {
  const retryAfter = Date.now() + delay;
  state.translationAutoRetryAfter.delete(key);
  state.translationAutoRetryAfter.set(key, retryAfter);
  while (state.translationAutoRetryAfter.size > MAX_AUTO_TRANSLATION_CACHE_ENTRIES) {
    state.translationAutoRetryAfter.delete(state.translationAutoRetryAfter.keys().next().value);
  }
}

function scheduleBuiltinTranslation(delay = BUILTIN_TRANSLATION_DELAY) {
  clearTimeout(state.translateTimer);
  state.translateTimer = null;
  const context = currentTranslationContext();
  if (context.targetLanguage === "zh-Hans") {
    setActiveAutoTranslations("");
    setTranslationStatus(
      t("translation.chineseStatus", "简体中文使用内置词典，无需自动翻译。"),
    );
    return;
  }
  if (!context.article) {
    setActiveAutoTranslations("");
    setTranslationStatus(
      t("translation.waitingForArticle", "输入英文文章后将自动使用内置翻译。"),
    );
    return;
  }
  if ($("#autoTranslations").value.trim()) {
    const count = $("#autoTranslations").value.split(/\r?\n/).filter(Boolean).length;
    setTranslationStatus(
      t("translation.cached", `已使用草稿中缓存的 ${count} 条多语释义。`, { count }),
    );
    return;
  }
  const retryAfter = state.translationAutoRetryAfter.get(context.key) || 0;
  if (retryAfter > Date.now()) {
    setTranslationStatus(localTranslationFallbackStatus());
    state.translateTimer = setTimeout(() => {
      state.translateTimer = null;
      if (context.key === currentTranslationContext().key) {
        state.translationAutoRetryAfter.delete(context.key);
        translateWithBuiltin();
      }
    }, Math.max(250, retryAfter - Date.now()));
    return;
  }
  state.translationAutoRetryAfter.delete(context.key);
  setTranslationStatus(t("translation.queued", "等待内置自动翻译…"));
  state.translateTimer = setTimeout(() => {
    state.translateTimer = null;
    translateWithBuiltin();
  }, delay);
}

async function translateWithBuiltin({ manual = false } = {}) {
  if (state.translateBusy) {
    return;
  }
  const context = currentTranslationContext();
  if (context.targetLanguage === "zh-Hans" || !context.article) {
    scheduleBuiltinTranslation();
    return;
  }

  const requestId = ++state.translateRequestId;
  const requestKey = context.key;
  if (!manual && (state.translationAutoRetryAfter.get(requestKey) || 0) > Date.now()) {
    setTranslationStatus(localTranslationFallbackStatus());
    return;
  }
  if (manual) state.translationAutoRetryAfter.delete(requestKey);
  const controller =
    typeof AbortController === "function" ? new AbortController() : null;
  state.translateAbortController = controller;
  setTranslateBusy(true);
  setTranslationStatus(t("translation.translatingStatus", "正在生成目标语释义…"));
  try {
    const payload = requestPayload(context.article);
    payload.customWords = $("#customWords").value;
    const response = await fetch("/api/builtin-translate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
      signal: controller?.signal,
    });
    const data = await response.json();
    const actualLanguage = String(data.actualLanguage || "");
    const usedFallback =
      data.fallback === true ||
      Boolean(data.warning) ||
      (actualLanguage && actualLanguage !== context.targetLanguage);
    const fallbackReason = String(data.reason || "");
    if ((!response.ok || !data.ok) && !usedFallback) {
      throw new Error("TRANSLATION_FAILED");
    }
    if (
      requestId !== state.translateRequestId ||
      requestKey !== currentTranslationContext().key ||
      controller?.signal.aborted
    ) {
      return;
    }

    if (usedFallback && fallbackReason === "busy") {
      const retryDelay = Math.max(
        1_000,
        Math.min(Number(data.retryAfterMs) || 2_000, 5_000),
      );
      setTranslationStatus(localTranslationFallbackStatus("busy"));
      clearTimeout(state.translateTimer);
      state.translateTimer = setTimeout(() => {
        state.translateTimer = null;
        if (requestKey === currentTranslationContext().key) {
          translateWithBuiltin();
        }
      }, retryDelay);
      return;
    }

    if (usedFallback) {
      const preservedExisting = Boolean($("#autoTranslations").value.trim());
      const retryDelay = Math.max(
        TRANSLATION_FAILURE_RETRY_DELAY,
        Math.min(
          Number(data.retryAfterMs) || TRANSLATION_FALLBACK_RETRY_DELAY,
          TRANSLATION_FALLBACK_RETRY_DELAY,
        ),
      );
      blockAutomaticTranslationRetry(requestKey, retryDelay);
      scheduleBuiltinTranslation();
      setTranslationStatus(
        localTranslationFallbackStatus(fallbackReason, preservedExisting),
      );
    } else {
      const annotations = String(data.annotations || "");
      const count = Number.isFinite(data.count)
        ? data.count
        : annotations.split(/\r?\n/).filter(Boolean).length;
      state.activeTranslationKey = requestKey;
      setActiveAutoTranslations(annotations);
      cacheAutoTranslations(requestKey, annotations);
      schedulePreview();
      state.translationAutoRetryAfter.delete(requestKey);
      setTranslationStatus(
        t("translation.completed", `内置翻译已生成 ${count} 条释义。`, { count }),
      );
    }
  } catch (error) {
    if (requestId !== state.translateRequestId) return;
    if (error?.name === "AbortError") return;
    blockAutomaticTranslationRetry(requestKey, TRANSLATION_FAILURE_RETRY_DELAY);
    scheduleBuiltinTranslation();
    const message = localTranslationFallbackStatus(
      "unavailable",
      Boolean($("#autoTranslations").value.trim()),
    );
    setTranslationStatus(message);
    showToast(message);
  } finally {
    if (requestId === state.translateRequestId) {
      state.translateAbortController = null;
      setTranslateBusy(false);
    }
  }
}

function wireEvents() {
  $("#themeBtn").addEventListener("click", toggleTheme);
  $("#demoBtn").addEventListener("click", loadDemo);
  $("#clearBtn").addEventListener("click", clearAll);
  $("#docxBtn").addEventListener("click", () => generateFile("docx"));
  $("#pdfBtn").addEventListener("click", () => generateFile("pdf"));
  $("#translateBtn").addEventListener("click", () => {
    clearTimeout(state.translateTimer);
    state.translateTimer = null;
    translateWithBuiltin({ manual: true });
  });
  $("#interfaceLanguage").addEventListener("change", () => {
    const select = $("#interfaceLanguage");
    const language = selectedInterfaceLanguageName(select.value);
    setInterfaceLanguageStatus(
      "uiLanguage.loading",
      `正在切换至${language}界面…`,
      select.value,
    );
  });
  document.addEventListener("yujie:localechange", (event) => {
    $("#interfaceLanguage").disabled = false;
    refreshLocaleDependentUi();
    refreshInterfaceLanguageStatus({
      failed: event.detail?.loaded === false,
      locale: event.detail?.requestedLocale,
    });
  });
  $("#resultCard").addEventListener("click", () => {
    if (window.__CIJING_DESKTOP__ && window.ipc) {
      window.ipc.postMessage("open-output-folder");
    }
  });
  $("#article").addEventListener("input", () => {
    updateWordCount();
    activateCurrentTranslationContext();
    schedulePreview();
    scheduleBuiltinTranslation();
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
    activateCurrentTranslationContext();
    schedulePreview();
    scheduleBuiltinTranslation();
  });
  $("#translationLanguage").addEventListener("change", () => {
    activateCurrentTranslationContext();
    updateLanguageUi({ notify: true });
    schedulePreview();
    scheduleBuiltinTranslation();
  });
  $("#pronunciationScheme").addEventListener("change", () => {
    updateLanguageUi();
    schedulePreview();
    showToast(t("pronunciation.updated", "注音方式已更新，本地预览已刷新"));
  });
  $("#customWords").addEventListener("input", () => {
    updateCustomPreview();
    activateCurrentTranslationContext();
    schedulePreview();
    scheduleBuiltinTranslation();
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
  await window.YujieI18n?.ready?.();
  window.YujieI18n?.applyDocument?.();
  $("#interfaceLanguage").value = window.YujieI18n?.getLocale?.() || "zh-Hans";
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
      state.activeTranslationKey = null;
      activateCurrentTranslationContext({ migrateLegacy: true });
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
  if (!restored) {
    activateCurrentTranslationContext({ migrateLegacy: true });
  }
  updateWordCount();
  updateCustomPreview();
  if (restored) {
    updateGrade();
    applyLayoutSettings();
  }
  updateLanguageUi();
  refreshInterfaceLanguageStatus();
  schedulePreview();
  scheduleBuiltinTranslation();
}

boot().catch((error) => showToast(error.message || "页面初始化失败"));
