(() => {
  "use strict";

  const DRAFT_KEY = "cijing-editor-draft-v2";
  const HISTORY_KEY = "cijing-editor-history-v2";
  const MAX_HISTORY = 20;
  const MAX_UNDO = 100;
  const HISTORY_INTERVAL = 45_000;
  const AUTOSAVE_DELAY = 700;
  const VALID_COLORS = new Set(["yellow", "green", "blue", "pink", "orange"]);
  const FIELD_IDS = [
    "title",
    "customWords",
    "grade",
    "annotateUnknown",
    "englishSize",
    "ipaSize",
    "zhSize",
    "lineHeight",
    "wordSpacing",
    "pageSize",
    "customPageWidth",
    "customPageHeight",
  ];
  const KIND_LABELS = {
    highlight: "高亮",
    word: "生词",
    important: "重点词",
    ignore: "忽略词",
    grammar: "语法解析",
    pattern: "句型结构",
    complex: "长难句",
    teacher: "教师批注",
    tip: "学习提示",
  };

  const editorState = {
    article: null,
    options: {},
    annotations: [],
    undoStack: [],
    redoStack: [],
    lastText: "",
    lastUndoAt: 0,
    lastSelection: null,
    pendingRange: null,
    draftTimer: null,
    lastHistoryAt: 0,
    resizeObserver: null,
    dictionaryResult: null,
  };

  const $ = (selector) => document.querySelector(selector);
  const $$ = (selector) => Array.from(document.querySelectorAll(selector));

  function escapeHtml(value) {
    return String(value)
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;");
  }

  function normalizeWord(value) {
    return String(value)
      .trim()
      .replace(/^[!*]/, "")
      .replace(/^[^A-Za-z]+|[^A-Za-z'-]+$/g, "")
      .toLowerCase();
  }

  function countWords(text) {
    return text.match(/[A-Za-z]+(?:[-'][A-Za-z]+)*/g)?.length || 0;
  }

  function updateStats() {
    const text = editorState.article.value;
    const words = countWords(text);
    const sentences = text
      .split(/[.!?]+(?:["')\]]+)?/)
      .map((part) => part.trim())
      .filter((part) => /[A-Za-z]/.test(part)).length;
    const paragraphs = text
      .split(/\n+/)
      .map((part) => part.trim())
      .filter(Boolean).length;
    const minutes = words ? Math.max(1, Math.ceil(words / 180)) : 0;
    $("#textStats").textContent =
      `${words} 词 · ${sentences} 句 · ${paragraphs} 段 · ${minutes} 分钟`;
  }

  function annotationId() {
    return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  }

  function validRange(range) {
    if (!range) return null;
    const length = editorState.article.value.length;
    const start = Math.max(0, Math.min(length, Number(range.start) || 0));
    const end = Math.max(start, Math.min(length, Number(range.end) || start));
    return end > start ? { start, end } : null;
  }

  function sanitizeAnnotations(items) {
    if (!Array.isArray(items)) return [];
    return items
      .map((item) => {
        const range = validRange(item);
        if (!range) return null;
        return {
          id: String(item.id || annotationId()),
          kind: KIND_LABELS[item.kind] ? item.kind : "highlight",
          start: range.start,
          end: range.end,
          color: VALID_COLORS.has(item.color) ? item.color : "yellow",
          term: String(item.term || ""),
          ipa: String(item.ipa || ""),
          definition: String(item.definition || ""),
          note: String(item.note || ""),
          excerpt: editorState.article.value.slice(range.start, range.end),
          createdAt: Number(item.createdAt) || Date.now(),
        };
      })
      .filter(Boolean);
  }

  function adjustAnnotations(oldText, newText) {
    if (oldText === newText || !editorState.annotations.length) return;
    let prefix = 0;
    while (
      prefix < oldText.length &&
      prefix < newText.length &&
      oldText[prefix] === newText[prefix]
    ) {
      prefix += 1;
    }
    let suffix = 0;
    while (
      suffix < oldText.length - prefix &&
      suffix < newText.length - prefix &&
      oldText[oldText.length - 1 - suffix] === newText[newText.length - 1 - suffix]
    ) {
      suffix += 1;
    }
    const oldEnd = oldText.length - suffix;
    const delta = newText.length - oldText.length;
    editorState.annotations = editorState.annotations
      .map((item) => {
        if (item.end <= prefix) return item;
        if (item.start >= oldEnd) {
          return {
            ...item,
            start: item.start + delta,
            end: item.end + delta,
          };
        }
        return null;
      })
      .filter(Boolean);
  }

  function renderHighlights() {
    const text = editorState.article.value;
    const content = $("#articleHighlightsContent");
    const colors = new Array(text.length);
    editorState.annotations.forEach((item) => {
      if (!VALID_COLORS.has(item.color)) return;
      const start = Math.max(0, Math.min(text.length, item.start));
      const end = Math.max(start, Math.min(text.length, item.end));
      for (let index = start; index < end; index += 1) {
        colors[index] = item.color;
      }
    });

    const output = [];
    let index = 0;
    while (index < text.length) {
      const color = colors[index] || "";
      let end = index + 1;
      while (end < text.length && (colors[end] || "") === color) end += 1;
      const segment = escapeHtml(text.slice(index, end));
      output.push(
        color ? `<mark class="editor-mark mark-${color}">${segment}</mark>` : segment,
      );
      index = end;
    }
    content.innerHTML = output.join("") || " ";
    syncHighlightGeometry();
  }

  function syncHighlightGeometry() {
    const article = editorState.article;
    const content = $("#articleHighlightsContent");
    content.style.width = `${article.clientWidth}px`;
    content.style.transform =
      `translate(${-article.scrollLeft}px, ${-article.scrollTop}px)`;
  }

  function annotationTitle(item) {
    if (item.term) return `${KIND_LABELS[item.kind] || "标注"} · ${item.term}`;
    return KIND_LABELS[item.kind] || "标注";
  }

  function renderAnnotationList() {
    const list = $("#annotationList");
    $("#annotationCount").textContent = String(editorState.annotations.length);
    list.innerHTML = "";
    if (!editorState.annotations.length) {
      const empty = document.createElement("p");
      empty.className = "annotation-empty";
      empty.textContent = "选择单词、句子或段落后即可添加标注。";
      list.appendChild(empty);
      return;
    }

    [...editorState.annotations]
      .sort((a, b) => a.start - b.start || a.createdAt - b.createdAt)
      .forEach((item) => {
        const row = document.createElement("article");
        row.className = "annotation-item";
        const swatch = document.createElement("span");
        swatch.className = `annotation-swatch ${item.color}`;
        const main = document.createElement("div");
        main.className = "annotation-item-main";
        main.tabIndex = 0;
        const title = document.createElement("strong");
        title.textContent = annotationTitle(item);
        const excerpt = document.createElement("p");
        excerpt.textContent = `“${item.excerpt || editorState.article.value.slice(item.start, item.end)}”`;
        const note = document.createElement("small");
        note.textContent = item.note || item.definition || "无附加备注";
        main.append(title, excerpt, note);
        main.addEventListener("click", () => selectAnnotation(item));
        main.addEventListener("keydown", (event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            selectAnnotation(item);
          }
        });
        const remove = document.createElement("button");
        remove.type = "button";
        remove.className = "annotation-delete";
        remove.setAttribute("aria-label", "删除标注");
        remove.textContent = "×";
        remove.addEventListener("click", () => {
          editorState.annotations = editorState.annotations.filter(
            (candidate) => candidate.id !== item.id,
          );
          renderAllAnnotations();
          scheduleDraftSave();
        });
        row.append(swatch, main, remove);
        list.appendChild(row);
      });
  }

  function renderAllAnnotations() {
    renderHighlights();
    renderAnnotationList();
    applyPreviewAnnotations();
  }

  function selectAnnotation(item) {
    editorState.article.focus();
    editorState.article.setSelectionRange(item.start, item.end);
    editorState.lastSelection = { start: item.start, end: item.end };
    updateSelectionBar();
    const textBefore = editorState.article.value.slice(0, item.start);
    const lineCount = textBefore.split("\n").length;
    editorState.article.scrollTop = Math.max(0, (lineCount - 2) * 31);
    syncHighlightGeometry();
  }

  function pushUndo(value = editorState.article.value) {
    if (editorState.undoStack.at(-1) === value) return;
    editorState.undoStack.push(value);
    if (editorState.undoStack.length > MAX_UNDO) editorState.undoStack.shift();
    editorState.redoStack.length = 0;
  }

  function dispatchArticleInput() {
    editorState.article.dispatchEvent(new Event("input", { bubbles: true }));
  }

  function setText(value, options = {}) {
    const next = String(value ?? "");
    if (next === editorState.article.value) return;
    if (options.record !== false) pushUndo();
    editorState.article.value = next;
    dispatchArticleInput();
  }

  function undo() {
    const previous = editorState.undoStack.pop();
    if (previous === undefined) return;
    editorState.redoStack.push(editorState.article.value);
    editorState.article.value = previous;
    dispatchArticleInput();
    editorState.article.focus();
    editorState.article.setSelectionRange(previous.length, previous.length);
  }

  function redo() {
    const next = editorState.redoStack.pop();
    if (next === undefined) return;
    editorState.undoStack.push(editorState.article.value);
    editorState.article.value = next;
    dispatchArticleInput();
    editorState.article.focus();
    editorState.article.setSelectionRange(next.length, next.length);
  }

  function articleInput() {
    const current = editorState.article.value;
    adjustAnnotations(editorState.lastText, current);
    editorState.lastText = current;
    updateStats();
    renderAllAnnotations();
    updateSelectionBar();
    scheduleDraftSave();
  }

  function articleBeforeInput(event) {
    if (event.inputType?.startsWith("history")) return;
    const now = Date.now();
    const immediate =
      event.inputType?.includes("Paste") ||
      event.inputType?.includes("Drop") ||
      event.inputType?.startsWith("delete");
    if (immediate || now - editorState.lastUndoAt > 650) {
      pushUndo();
      editorState.lastUndoAt = now;
    }
  }

  function findWordRange(position, preferredRange = null) {
    const text = editorState.article.value;
    if (preferredRange && preferredRange.end > preferredRange.start) {
      const selected = text.slice(preferredRange.start, preferredRange.end);
      const match = /[A-Za-z]+(?:[-'][A-Za-z]+)*/.exec(selected);
      if (match) {
        return {
          start: preferredRange.start + match.index,
          end: preferredRange.start + match.index + match[0].length,
        };
      }
    }
    const regex = /[A-Za-z]+(?:[-'][A-Za-z]+)*/g;
    let match;
    while ((match = regex.exec(text))) {
      const start = match.index;
      const end = start + match[0].length;
      if (position >= start && position <= end) return { start, end };
    }
    return null;
  }

  function sentenceRange(range) {
    const text = editorState.article.value;
    const startAt = range?.start ?? editorState.article.selectionStart;
    const endAt = range?.end ?? startAt;
    let start = startAt;
    while (start > 0 && !/[.!?\n]/.test(text[start - 1])) start -= 1;
    while (start < text.length && /\s/.test(text[start])) start += 1;
    let end = endAt;
    while (end < text.length && !/[.!?]/.test(text[end])) end += 1;
    if (end < text.length) {
      end += 1;
      while (end < text.length && /["')\]]/.test(text[end])) end += 1;
    }
    return end > start ? { start, end } : null;
  }

  function paragraphRange(range) {
    const text = editorState.article.value;
    const startAt = range?.start ?? editorState.article.selectionStart;
    const endAt = range?.end ?? startAt;
    const before = text.lastIndexOf("\n\n", Math.max(0, startAt - 1));
    let start = before < 0 ? 0 : before + 2;
    const after = text.indexOf("\n\n", endAt);
    let end = after < 0 ? text.length : after;
    while (start < end && /\s/.test(text[start])) start += 1;
    while (end > start && /\s/.test(text[end - 1])) end -= 1;
    return end > start ? { start, end } : null;
  }

  function captureSelection() {
    const start = editorState.article.selectionStart;
    const end = editorState.article.selectionEnd;
    editorState.lastSelection =
      end > start ? { start, end } : findWordRange(start, { start, end });
    updateSelectionBar();
  }

  function updateSelectionBar() {
    const bar = $("#selectionBar");
    const range = validRange(editorState.lastSelection);
    if (!range) {
      bar.hidden = true;
      return;
    }
    const excerpt = editorState.article.value.slice(range.start, range.end).trim();
    if (!excerpt) {
      bar.hidden = true;
      return;
    }
    $("#selectionSummary").textContent =
      excerpt.length > 22 ? `${excerpt.slice(0, 22)}…` : excerpt;
    bar.hidden = false;
  }

  function addAnnotation(data) {
    const range = validRange(data);
    if (!range) return;
    const item = {
      id: annotationId(),
      kind: KIND_LABELS[data.kind] ? data.kind : "highlight",
      start: range.start,
      end: range.end,
      color: VALID_COLORS.has(data.color) ? data.color : "yellow",
      term: String(data.term || ""),
      ipa: String(data.ipa || ""),
      definition: String(data.definition || ""),
      note: String(data.note || ""),
      excerpt: editorState.article.value.slice(range.start, range.end),
      createdAt: Date.now(),
    };
    editorState.annotations.push(item);
    renderAllAnnotations();
    scheduleDraftSave();
  }

  function directHighlight(color) {
    const range = validRange(editorState.lastSelection);
    if (!range) {
      editorState.options.showToast?.("请先选择需要高亮的内容");
      return;
    }
    addAnnotation({ ...range, kind: "highlight", color });
    editorState.options.showToast?.("已添加高亮");
  }

  function openDialog(dialog) {
    if (!dialog.open) dialog.showModal();
  }

  async function lookupWord(word) {
    const response = await fetch("/api/dictionary", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ word }),
    });
    const data = await response.json();
    if (!response.ok || !data.ok) throw new Error(data.error || "词典查询失败");
    return data;
  }

  async function fillWordLookup(word) {
    const status = $("#wordLookupStatus");
    status.textContent = "正在查询本地词典…";
    try {
      const data = await lookupWord(word);
      if (data.found) {
        if (!$("#wordIpa").value) $("#wordIpa").value = data.ipa || "";
        if (!$("#wordDefinition").value) {
          $("#wordDefinition").value = data.definition || "";
        }
        status.textContent = data.term && normalizeWord(data.term) !== normalizeWord(word)
          ? `已按原形 ${data.term} 查询；可能原形：${(data.forms || []).join("、")}`
          : "已从本地词典补全";
      } else {
        status.textContent = "本地词典暂未收录，可手动填写或使用右侧网络词库。";
      }
      return data;
    } catch (error) {
      status.textContent = error.message || "词典查询失败";
      return null;
    }
  }

  function openWordDialog(range = null) {
    const selected = validRange(range || editorState.lastSelection);
    const wordRange = findWordRange(
      selected?.start ?? editorState.article.selectionStart,
      selected,
    );
    if (!wordRange) {
      editorState.options.showToast?.("请先选择一个英文单词");
      return;
    }
    editorState.pendingRange = wordRange;
    const word = editorState.article.value.slice(wordRange.start, wordRange.end);
    $("#wordTerm").value = word;
    $("#wordStatus").value = "new";
    $("#wordIpa").value = "";
    $("#wordDefinition").value = "";
    $("#wordColor").value = "yellow";
    $("#wordNote").value = "";
    $("#wordLookupStatus").textContent = "";
    openDialog($("#wordDialog"));
    fillWordLookup(word);
  }

  function vocabularyEntry(line) {
    let value = String(line).trim();
    if (!value) return null;
    let status = "new";
    if (value.startsWith("!")) {
      status = "ignore";
      value = value.slice(1).trim();
    } else if (value.startsWith("*")) {
      status = "important";
      value = value.slice(1).trim();
    }
    const separator = ["=", "|", ":"].find((candidate) => value.includes(candidate));
    if (!separator) {
      return { word: value, ipa: "", definition: "", status };
    }
    const parts = value.split(separator);
    const word = (parts.shift() || "").trim();
    const ipa = (parts.shift() || "").trim();
    const definition = parts.join(separator).trim();
    return word ? { word, ipa, definition, status } : null;
  }

  function parseVocabulary() {
    return $("#customWords")
      .value.split(/\n/)
      .map(vocabularyEntry)
      .filter(Boolean);
  }

  function formatVocabularyEntry(entry) {
    const word = entry.word.trim();
    if (!word) return "";
    if (entry.status === "ignore") return `!${word}`;
    const prefix = entry.status === "important" ? "*" : "";
    if (entry.ipa.trim() && entry.definition.trim()) {
      return `${prefix}${word}=${entry.ipa.trim()}=${entry.definition.trim()}`;
    }
    return `${prefix}${word}`;
  }

  function upsertVocabulary(entry) {
    const key = normalizeWord(entry.word);
    const rows = parseVocabulary().filter(
      (candidate) => normalizeWord(candidate.word) !== key,
    );
    rows.push(entry);
    $("#customWords").value = rows.map(formatVocabularyEntry).filter(Boolean).join("\n");
    $("#customWords").dispatchEvent(new Event("input", { bubbles: true }));
  }

  function saveWordAnnotation(event) {
    event.preventDefault();
    const range = validRange(editorState.pendingRange);
    const term = $("#wordTerm").value.trim();
    if (!term) return;
    const status = $("#wordStatus").value;
    const ipa = $("#wordIpa").value.trim();
    const definition = $("#wordDefinition").value.trim();
    const note = $("#wordNote").value.trim();
    const color = $("#wordColor").value;
    upsertVocabulary({ word: term, ipa, definition, status });

    if (range) {
      editorState.annotations = editorState.annotations.filter(
        (item) =>
          !(
            item.start === range.start &&
            item.end === range.end &&
            ["word", "important", "ignore"].includes(item.kind)
          ),
      );
      addAnnotation({
        ...range,
        kind: status === "new" ? "word" : status,
        color,
        term,
        ipa,
        definition,
        note,
      });
    }
    $("#wordDialog").close();
    editorState.options.showToast?.(
      status === "ignore" ? "已设为忽略词" : "词汇标注已保存",
    );
  }

  function openNoteDialog(scope) {
    const selected = validRange(editorState.lastSelection);
    const range = scope === "paragraph" ? paragraphRange(selected) : sentenceRange(selected);
    if (!range) {
      editorState.options.showToast?.("请先将光标放入需要标注的内容");
      return;
    }
    editorState.pendingRange = range;
    const excerpt = editorState.article.value.slice(range.start, range.end).trim();
    $("#noteDialogTitle").textContent =
      scope === "paragraph" ? "添加段落批注" : "添加句子解析";
    $("#noteExcerpt").textContent = excerpt.length > 120 ? `${excerpt.slice(0, 120)}…` : excerpt;
    $("#noteKind").value = scope === "paragraph" ? "teacher" : "grammar";
    $("#noteColor").value = scope === "paragraph" ? "green" : "blue";
    $("#noteText").value = "";
    openDialog($("#noteDialog"));
  }

  function saveNote(event) {
    event.preventDefault();
    const range = validRange(editorState.pendingRange);
    const note = $("#noteText").value.trim();
    if (!range || !note) return;
    addAnnotation({
      ...range,
      kind: $("#noteKind").value,
      color: $("#noteColor").value,
      note,
    });
    $("#noteDialog").close();
    editorState.options.showToast?.("解析或批注已保存");
  }

  async function showDictionary(word, range, x, y) {
    const popover = $("#dictionaryPopover");
    editorState.pendingRange = range;
    editorState.dictionaryResult = null;
    $("#dictionaryWord").textContent = word;
    $("#dictionaryIpa").textContent = "";
    $("#dictionaryDefinition").textContent = "正在查询本地词典…";
    $("#dictionaryForms").textContent = "";
    popover.hidden = false;
    const width = 330;
    popover.style.left = `${Math.max(12, Math.min(window.innerWidth - width - 12, x))}px`;
    popover.style.top = `${Math.max(12, Math.min(window.innerHeight - 250, y))}px`;
    try {
      const data = await lookupWord(word);
      editorState.dictionaryResult = data;
      $("#dictionaryIpa").textContent = data.ipa || "";
      $("#dictionaryDefinition").textContent = data.found
        ? data.definition || "词典中暂无中文释义"
        : "本地词典暂未收录，可加入词汇标注后手动填写。";
      $("#dictionaryForms").textContent = data.forms?.length
        ? `可能原形：${data.forms.join("、")}`
        : "";
    } catch (error) {
      $("#dictionaryDefinition").textContent = error.message || "词典查询失败";
    }
  }

  function contextDictionary(event) {
    const position = editorState.article.selectionStart;
    const preferred = {
      start: editorState.article.selectionStart,
      end: editorState.article.selectionEnd,
    };
    const range = findWordRange(position, preferred);
    if (!range) return;
    event.preventDefault();
    editorState.lastSelection = range;
    editorState.article.setSelectionRange(range.start, range.end);
    updateSelectionBar();
    showDictionary(
      editorState.article.value.slice(range.start, range.end),
      range,
      event.clientX,
      event.clientY,
    );
  }

  function openDictionaryForSelection() {
    const selected = validRange(editorState.lastSelection);
    const range = findWordRange(selected?.start ?? editorState.article.selectionStart, selected);
    if (!range) {
      editorState.options.showToast?.("请先选择一个英文单词");
      return;
    }
    showDictionary(
      editorState.article.value.slice(range.start, range.end),
      range,
      Math.min(window.innerWidth - 350, 28),
      120,
    );
  }

  function addVocabularyRow(entry = {}) {
    const row = document.createElement("tr");
    const fields = [
      ["word", "text", entry.word || ""],
      ["ipa", "text", entry.ipa || ""],
      ["definition", "text", entry.definition || ""],
    ];
    fields.forEach(([name, type, value]) => {
      const cell = document.createElement("td");
      const input = document.createElement("input");
      input.type = type;
      input.dataset.vocabField = name;
      input.value = value;
      cell.appendChild(input);
      row.appendChild(cell);
    });
    const statusCell = document.createElement("td");
    const select = document.createElement("select");
    select.dataset.vocabField = "status";
    [
      ["new", "生词"],
      ["important", "重点词"],
      ["ignore", "忽略"],
    ].forEach(([value, label]) => {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = label;
      select.appendChild(option);
    });
    select.value = entry.status || "new";
    statusCell.appendChild(select);
    row.appendChild(statusCell);
    const actionCell = document.createElement("td");
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "tool-button remove-vocab-row";
    remove.textContent = "×";
    remove.setAttribute("aria-label", "删除词条");
    remove.addEventListener("click", () => row.remove());
    actionCell.appendChild(remove);
    row.appendChild(actionCell);
    $("#vocabularyRows").appendChild(row);
  }

  function openVocabularyManager() {
    const body = $("#vocabularyRows");
    body.innerHTML = "";
    parseVocabulary().forEach(addVocabularyRow);
    if (!body.children.length) addVocabularyRow();
    openDialog($("#vocabularyDialog"));
  }

  function saveVocabulary(event) {
    event.preventDefault();
    const entries = $$("#vocabularyRows tr")
      .map((row) => {
        const value = (name) =>
          row.querySelector(`[data-vocab-field="${name}"]`)?.value.trim() || "";
        return {
          word: value("word"),
          ipa: value("ipa"),
          definition: value("definition"),
          status: value("status") || "new",
        };
      })
      .filter((entry) => entry.word);
    const entryByWord = new Map(
      entries.map((entry) => [normalizeWord(entry.word), entry]),
    );
    editorState.annotations = editorState.annotations.map((item) => {
      const entry = entryByWord.get(normalizeWord(item.term));
      if (!entry) return item;
      return {
        ...item,
        kind: entry.status === "new" ? "word" : entry.status,
        ipa: entry.ipa,
        definition: entry.definition,
      };
    });
    $("#customWords").value = entries.map(formatVocabularyEntry).filter(Boolean).join("\n");
    $("#customWords").dispatchEvent(new Event("input", { bubbles: true }));
    renderAllAnnotations();
    scheduleDraftSave();
    $("#vocabularyDialog").close();
    editorState.options.showToast?.(`已保存 ${entries.length} 个词条`);
  }

  function cleanArticleText(text) {
    const lines = text
      .replace(/\r\n?/g, "\n")
      .replace(/\u00a0/g, " ")
      .replace(/\t/g, "    ")
      .split("\n")
      .map((line) => line.trim().replace(/[ \t]{2,}/g, " "));
    const normalized = [];
    lines.forEach((line) => {
      if (!line && !normalized.at(-1)) return;
      const previous = normalized.at(-1);
      if (
        line &&
        previous &&
        !/[.!?;:”“"')\]]$/.test(previous) &&
        (/^[a-z]/.test(line) || previous.endsWith("-"))
      ) {
        normalized[normalized.length - 1] = previous.endsWith("-")
          ? `${previous.slice(0, -1)}${line}`
          : `${previous} ${line}`;
      } else {
        normalized.push(line);
      }
    });
    return normalized.join("\n").trim();
  }

  function smartQuotes(text) {
    let doubleOpen = true;
    let singleOpen = true;
    return Array.from(text)
      .map((character, index, chars) => {
        if (character === "\n") {
          doubleOpen = true;
          singleOpen = true;
          return character;
        }
        if (character === '"') {
          const output = doubleOpen ? "“" : "”";
          doubleOpen = !doubleOpen;
          return output;
        }
        if (character === "'") {
          const previous = chars[index - 1] || "";
          const next = chars[index + 1] || "";
          if (/[A-Za-z]/.test(previous) && /[A-Za-z]/.test(next)) return "'";
          const output = singleOpen ? "‘" : "’";
          singleOpen = !singleOpen;
          return output;
        }
        return character;
      })
      .join("");
  }

  function normalizeEnglishText(text) {
    let result = smartQuotes(text);
    result = result
      .replace(/[ \t]+([,.;:!?])/g, "$1")
      .replace(/([,;:!?])(?=[A-Za-z])/g, "$1 ")
      .replace(/([.!?]["”’)]*\s+)([a-z])/g, (_, prefix, letter) =>
        `${prefix}${letter.toUpperCase()}`,
      )
      .replace(/(^|\n)(\s*)([a-z])/g, (_, prefix, spaces, letter) =>
        `${prefix}${spaces}${letter.toUpperCase()}`,
      );
    return result;
  }

  function transformArticle(transform, label) {
    const current = editorState.article.value;
    const next = transform(current);
    if (next === current) {
      editorState.options.showToast?.("当前文本无需调整");
      return;
    }
    captureHistory(`${label}前`);
    setText(next);
    editorState.options.showToast?.(`${label}完成`);
  }

  function openFind() {
    $("#findPanel").hidden = false;
    const selected = editorState.article.value.slice(
      editorState.article.selectionStart,
      editorState.article.selectionEnd,
    );
    if (selected && !selected.includes("\n")) $("#findText").value = selected;
    $("#findText").focus();
    $("#findText").select();
    updateFindStatus();
  }

  function findMatches() {
    const query = $("#findText").value;
    if (!query) return [];
    const source = $("#findCaseSensitive").checked
      ? editorState.article.value
      : editorState.article.value.toLowerCase();
    const needle = $("#findCaseSensitive").checked ? query : query.toLowerCase();
    const matches = [];
    let index = 0;
    while ((index = source.indexOf(needle, index)) >= 0) {
      matches.push(index);
      index += Math.max(1, needle.length);
    }
    return matches;
  }

  function updateFindStatus() {
    const query = $("#findText").value;
    $("#findStatus").textContent = query ? `共 ${findMatches().length} 处` : "";
  }

  function findNext() {
    const query = $("#findText").value;
    if (!query) return;
    const matches = findMatches();
    if (!matches.length) {
      $("#findStatus").textContent = "未找到";
      return;
    }
    const after = editorState.article.selectionEnd;
    const start = matches.find((index) => index >= after) ?? matches[0];
    editorState.article.focus();
    editorState.article.setSelectionRange(start, start + query.length);
    editorState.lastSelection = { start, end: start + query.length };
    $("#findStatus").textContent = `第 ${matches.indexOf(start) + 1} / ${matches.length} 处`;
    updateSelectionBar();
  }

  function replaceCurrent() {
    const query = $("#findText").value;
    if (!query) return;
    const start = editorState.article.selectionStart;
    const end = editorState.article.selectionEnd;
    const selected = editorState.article.value.slice(start, end);
    const matches = $("#findCaseSensitive").checked
      ? selected === query
      : selected.toLowerCase() === query.toLowerCase();
    if (!matches) {
      findNext();
      return;
    }
    pushUndo();
    editorState.article.setRangeText($("#replaceText").value, start, end, "select");
    dispatchArticleInput();
    findNext();
  }

  function replaceAll() {
    const query = $("#findText").value;
    if (!query) return;
    const flags = $("#findCaseSensitive").checked ? "g" : "gi";
    const pattern = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), flags);
    const matches = editorState.article.value.match(pattern)?.length || 0;
    if (!matches) {
      $("#findStatus").textContent = "未找到";
      return;
    }
    captureHistory("全部替换前");
    setText(editorState.article.value.replace(pattern, () => $("#replaceText").value));
    $("#findStatus").textContent = `已替换 ${matches} 处`;
  }

  function collectDraft() {
    const fields = {};
    FIELD_IDS.forEach((id) => {
      const element = $(`#${id}`);
      if (!element) return;
      fields[id] = element.type === "checkbox" ? element.checked : element.value;
    });
    return {
      version: 2,
      updatedAt: Date.now(),
      article: editorState.article.value,
      annotations: editorState.annotations,
      fields,
    };
  }

  function readHistory() {
    try {
      const history = JSON.parse(localStorage.getItem(HISTORY_KEY) || "[]");
      return Array.isArray(history) ? history : [];
    } catch {
      return [];
    }
  }

  function draftSignature(draft) {
    return JSON.stringify({
      article: draft.article,
      title: draft.fields?.title,
      customWords: draft.fields?.customWords,
      annotations: draft.annotations,
    });
  }

  function captureHistory(reason = "自动版本") {
    const draft = collectDraft();
    if (!draft.article.trim() && !draft.fields.title?.trim()) return;
    const history = readHistory();
    if (history[0] && draftSignature(history[0].draft) === draftSignature(draft)) return;
    history.unshift({
      id: annotationId(),
      timestamp: Date.now(),
      reason,
      draft,
    });
    localStorage.setItem(HISTORY_KEY, JSON.stringify(history.slice(0, MAX_HISTORY)));
    editorState.lastHistoryAt = Date.now();
  }

  function saveDraft() {
    clearTimeout(editorState.draftTimer);
    try {
      localStorage.setItem(DRAFT_KEY, JSON.stringify(collectDraft()));
      const status = $("#draftStatus");
      status.className = "saved";
      status.textContent = `已自动保存 ${new Date().toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      })}`;
      if (Date.now() - editorState.lastHistoryAt > HISTORY_INTERVAL) {
        captureHistory();
      }
    } catch {
      $("#draftStatus").className = "";
      $("#draftStatus").textContent = "草稿保存失败";
    }
  }

  function scheduleDraftSave() {
    const status = $("#draftStatus");
    status.className = "saving";
    status.textContent = "正在保存…";
    clearTimeout(editorState.draftTimer);
    editorState.draftTimer = setTimeout(saveDraft, AUTOSAVE_DELAY);
  }

  function applyDraft(draft) {
    if (!draft || typeof draft !== "object") return false;
    FIELD_IDS.forEach((id) => {
      const element = $(`#${id}`);
      if (!element || draft.fields?.[id] === undefined) return;
      if (element.type === "checkbox") {
        element.checked = Boolean(draft.fields[id]);
      } else {
        element.value = String(draft.fields[id]);
      }
    });
    editorState.article.value = String(draft.article || "");
    editorState.lastText = editorState.article.value;
    editorState.annotations = sanitizeAnnotations(draft.annotations);
    editorState.undoStack.length = 0;
    editorState.redoStack.length = 0;
    updateStats();
    renderAllAnnotations();
    $("#draftStatus").className = "saved";
    $("#draftStatus").textContent = "已恢复草稿";
    editorState.options.onRestore?.();
    return true;
  }

  function restoreDraft() {
    try {
      const draft = JSON.parse(localStorage.getItem(DRAFT_KEY) || "null");
      return applyDraft(draft);
    } catch {
      return false;
    }
  }

  function renderHistory() {
    const list = $("#historyList");
    const history = readHistory();
    list.innerHTML = "";
    if (!history.length) {
      const empty = document.createElement("p");
      empty.className = "history-empty";
      empty.textContent = "尚未生成历史版本。继续编辑后会自动保存。";
      list.appendChild(empty);
      return;
    }
    history.forEach((entry) => {
      const row = document.createElement("article");
      row.className = "history-item";
      const main = document.createElement("div");
      const title = document.createElement("strong");
      title.textContent = entry.draft?.fields?.title?.trim() || "未命名文章";
      const preview = document.createElement("p");
      preview.textContent =
        String(entry.draft?.article || "").replace(/\s+/g, " ").slice(0, 100) ||
        "空白草稿";
      const meta = document.createElement("small");
      meta.textContent =
        `${new Date(entry.timestamp).toLocaleString()} · ${entry.reason || "自动版本"} · ` +
        `${countWords(entry.draft?.article || "")} 词`;
      main.append(title, preview, meta);
      const actions = document.createElement("div");
      actions.className = "history-actions";
      const restore = document.createElement("button");
      restore.type = "button";
      restore.className = "tool-button";
      restore.textContent = "恢复";
      restore.addEventListener("click", () => {
        captureHistory("恢复前");
        applyDraft(entry.draft);
        saveDraft();
        $("#historyDialog").close();
        editorState.options.showToast?.("历史版本已恢复");
      });
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "tool-button icon-only";
      remove.textContent = "×";
      remove.setAttribute("aria-label", "删除历史版本");
      remove.addEventListener("click", () => {
        const next = readHistory().filter((candidate) => candidate.id !== entry.id);
        localStorage.setItem(HISTORY_KEY, JSON.stringify(next));
        renderHistory();
      });
      actions.append(restore, remove);
      row.append(main, actions);
      list.appendChild(row);
    });
  }

  function openHistory() {
    saveDraft();
    renderHistory();
    openDialog($("#historyDialog"));
  }

  function applyPreviewAnnotations() {
    const articleWords = Array.from(
      editorState.article.value.matchAll(/[A-Za-z]+(?:[-'][A-Za-z]+)*/g),
    );
    const previewWords = $$(
      "#previewCanvas .preview-line:not(.preview-title) .preview-base",
    );
    previewWords.forEach((element) => {
      element.classList.remove(
        "editor-preview-mark",
        "mark-yellow",
        "mark-green",
        "mark-blue",
        "mark-pink",
        "mark-orange",
      );
      element.removeAttribute("title");
    });

    let previewIndex = 0;
    articleWords.forEach((match) => {
      const expected = normalizeWord(match[0]);
      while (
        previewIndex < previewWords.length &&
        normalizeWord(previewWords[previewIndex].textContent) !== expected
      ) {
        previewIndex += 1;
      }
      if (previewIndex >= previewWords.length) return;
      const element = previewWords[previewIndex];
      previewIndex += 1;
      const start = match.index;
      const end = start + match[0].length;
      const item = editorState.annotations.reduce(
        (active, candidate) =>
          candidate.kind !== "ignore" &&
          candidate.start < end &&
          candidate.end > start
            ? candidate
            : active,
        null,
      );
      if (!item) return;
      element.classList.add("editor-preview-mark", `mark-${item.color}`);
      const detail = item.note || item.definition;
      if (detail) element.title = detail;
    });
  }

  function toolbarAction(event) {
    const action = event.currentTarget.dataset.editorAction;
    if (action === "undo") undo();
    if (action === "redo") redo();
    if (action === "find") openFind();
    if (action === "history") openHistory();
    if (action === "cleanup") {
      transformArticle(cleanArticleText, "格式清理");
    }
    if (action === "normalize") {
      transformArticle(normalizeEnglishText, "英文规范");
    }
    if (action === "vocabulary") openVocabularyManager();
  }

  function selectionAction(event) {
    const action = event.currentTarget.dataset.selectionAction;
    if (action === "word") openWordDialog();
    if (action === "dictionary") openDictionaryForSelection();
    if (action === "sentence") openNoteDialog("sentence");
    if (action === "paragraph") openNoteDialog("paragraph");
  }

  function keyboardShortcuts(event) {
    const modifier = event.ctrlKey || event.metaKey;
    if (!modifier) return;
    const key = event.key.toLowerCase();
    if (key === "z" && !event.shiftKey) {
      event.preventDefault();
      undo();
    } else if (key === "y" || (key === "z" && event.shiftKey)) {
      event.preventDefault();
      redo();
    } else if (key === "f") {
      event.preventDefault();
      openFind();
    } else if (key === "s") {
      event.preventDefault();
      saveDraft();
      editorState.options.showToast?.("草稿已保存");
    }
  }

  function wireEditorEvents() {
    editorState.article.addEventListener("beforeinput", articleBeforeInput);
    editorState.article.addEventListener("input", articleInput);
    editorState.article.addEventListener("select", captureSelection);
    editorState.article.addEventListener("mouseup", captureSelection);
    editorState.article.addEventListener("keyup", captureSelection);
    editorState.article.addEventListener("scroll", syncHighlightGeometry);
    editorState.article.addEventListener("contextmenu", contextDictionary);
    document.addEventListener("keydown", keyboardShortcuts);

    $$("[data-editor-action]").forEach((button) =>
      button.addEventListener("click", toolbarAction),
    );
    $$("[data-selection-action]").forEach((button) => {
      button.addEventListener("mousedown", (event) => event.preventDefault());
      button.addEventListener("click", selectionAction);
    });
    $$("[data-highlight-color]").forEach((button) => {
      button.addEventListener("mousedown", (event) => event.preventDefault());
      button.addEventListener("click", () =>
        directHighlight(button.dataset.highlightColor),
      );
    });

    $("#findText").addEventListener("input", updateFindStatus);
    $("#findText").addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        findNext();
      }
      if (event.key === "Escape") $("#findPanel").hidden = true;
    });
    $("#findNextBtn").addEventListener("click", findNext);
    $("#replaceBtn").addEventListener("click", replaceCurrent);
    $("#replaceAllBtn").addEventListener("click", replaceAll);
    $("#findCloseBtn").addEventListener("click", () => {
      $("#findPanel").hidden = true;
      editorState.article.focus();
    });

    $("#wordForm").addEventListener("submit", saveWordAnnotation);
    $("#noteForm").addEventListener("submit", saveNote);
    $("#vocabularyForm").addEventListener("submit", saveVocabulary);
    $("#addVocabularyRowBtn").addEventListener("click", () => addVocabularyRow());
    $$("[data-dialog-close]").forEach((button) => {
      button.addEventListener("click", () => button.closest("dialog")?.close());
    });

    $("#dictionaryCloseBtn").addEventListener("click", () => {
      $("#dictionaryPopover").hidden = true;
    });
    $("#dictionaryAnnotateBtn").addEventListener("click", () => {
      $("#dictionaryPopover").hidden = true;
      openWordDialog(editorState.pendingRange);
      const result = editorState.dictionaryResult;
      if (result?.found) {
        $("#wordIpa").value = result.ipa || "";
        $("#wordDefinition").value = result.definition || "";
      }
    });
    document.addEventListener("pointerdown", (event) => {
      const popover = $("#dictionaryPopover");
      if (!popover.hidden && !popover.contains(event.target)) popover.hidden = true;
    });

    FIELD_IDS.forEach((id) => {
      const element = $(`#${id}`);
      if (!element || element === editorState.article) return;
      element.addEventListener("input", scheduleDraftSave);
      element.addEventListener("change", scheduleDraftSave);
    });

    if ("ResizeObserver" in window) {
      editorState.resizeObserver = new ResizeObserver(syncHighlightGeometry);
      editorState.resizeObserver.observe(editorState.article);
    }
  }

  function init(options = {}) {
    editorState.options = options;
    editorState.article = $("#article");
    editorState.lastText = editorState.article.value;
    wireEditorEvents();
    updateStats();
    renderAllAnnotations();
    return {
      setText,
      undo,
      redo,
      saveNow: saveDraft,
      restoreDraft,
      captureHistory,
      clearAnnotations() {
        editorState.annotations = [];
        renderAllAnnotations();
        scheduleDraftSave();
      },
      beforeDestructive(reason = "修改前") {
        captureHistory(reason);
      },
      notifyFieldsChanged: scheduleDraftSave,
      applyPreviewAnnotations,
      openVocabularyManager,
    };
  }

  window.CijingEditorTools = { init };
})();
