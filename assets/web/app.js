const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => Array.from(document.querySelectorAll(selector));

const PAGE_SIZES = {
  letter: { width: "8.5in", height: "11in" },
  a4: { width: "210mm", height: "297mm" },
  b5: { width: "176mm", height: "250mm" },
  a5: { width: "148mm", height: "210mm" },
};

const state = {
  profiles: [],
  profileByCode: new Map(),
  pdfBusy: false,
  enhanceBusy: false,
  lexiconBusy: false,
  previewBusy: false,
  previewQueued: false,
  previewTimer: null,
  toastTimer: null,
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
  $("#article").value = value;
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
  $("#title").value = data.title;
  setArticleText(data.text);
  $("#customWords").value = "glittered=ˈɡlɪt.ərd=闪闪发光";
  $("#resultCard").hidden = true;
  updateWordCount();
  updateCustomPreview();
  schedulePreview();
  showToast("已插入演示文本");
}

function clearAll() {
  $("#title").value = "";
  setArticleText("");
  $("#customWords").value = "";
  $("#resultCard").hidden = true;
  updateWordCount();
  updateCustomPreview();
  setPreviewEmpty("粘贴英文文章后自动生成预览");
  showToast("已清空");
}

function setPdfBusy(value) {
  state.pdfBusy = value;
  $("#pdfBtn").classList.toggle("busy", value);
  $("#pdfLabel").textContent = value ? "下载中..." : "下载 PDF";
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

function downloadGeneratedPdf(downloadUrl, filename) {
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

async function generatePdf() {
  if (state.pdfBusy) return;
  const article = $("#article").value.trim();
  if (!article) {
    showToast("请先粘贴英文文章");
    $("#article").focus();
    return;
  }

  setPdfBusy(true);
  $("#resultCard").hidden = true;
  try {
    const response = await fetch("/api/generate-pdf", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(requestPayload(article)),
    });
    const data = await response.json();
    if (!response.ok || !data.ok) throw new Error(data.error || "PDF 生成失败");

    downloadGeneratedPdf(data.downloadUrl, data.filename);
    $("#resultTitle").textContent = "PDF 已生成";
    $("#resultMeta").textContent = data.missingCount
      ? `已开始下载 ${data.filename}；有 ${data.missingCount} 个未收录词保持原文。`
      : `已开始下载 ${data.filename}。`;
    $("#resultCard").hidden = false;
    const menu = $(".output-menu");
    if (menu) menu.open = false;
    showToast("PDF 已生成，已开始下载");
  } catch (error) {
    showToast(error.message || "PDF 输出失败");
  } finally {
    setPdfBusy(false);
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
  $("#pdfBtn").addEventListener("click", generatePdf);
  $("#enhanceBtn").addEventListener("click", enhanceAnnotations);
  $("#lexiconBtn").addEventListener("click", enhanceFromNetworkLexicon);
  $("#article").addEventListener("input", () => {
    updateWordCount();
    schedulePreview();
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
  $$(".range-field input, #pageSize, #customPageWidth, #customPageHeight").forEach((control) => {
    control.addEventListener("input", applyLayoutSettings);
    control.addEventListener("change", applyLayoutSettings);
  });
}

async function boot() {
  applyTheme(localStorage.getItem("cijing-theme") || "light");
  wireEvents();
  applyLayoutSettings();
  await loadProfiles();
  updateWordCount();
  updateCustomPreview();
  schedulePreview();
}

boot().catch((error) => showToast(error.message || "页面初始化失败"));
