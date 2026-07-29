(function (global) {
  "use strict";

  const STORAGE_KEY = "yujie-ui-locale-v1";
  const DEFAULT_LOCALE = "zh-Hans";
  const RTL_LOCALES = new Set(["ar", "fa", "he", "ur", "ps", "sd", "ug"]);
  const BUILT_IN_LOCALES = new Set(["zh-Hans", "en"]);

  const MESSAGES = Object.freeze({
    "app.name": { "zh-CN": "语界精读", en: "语界精读 · Yujie Reader" },
    "app.description": {
      "zh-CN": "语界精读——支持多语言翻译、智能注音与分级词汇标注的精读工具。",
      en: "Yujie Reader — an intensive-reading tool with multilingual definitions, pronunciation guides, and level-based vocabulary annotation.",
    },
    "app.tagline": { "zh-CN": "在语境中，读懂世界", en: "Read the world through context" },
    "action.dark": { "zh-CN": "深色", en: "Dark" },
    "action.light": { "zh-CN": "浅色", en: "Light" },
    "action.demo": { "zh-CN": "演示", en: "Demo" },
    "action.clear": { "zh-CN": "清空", en: "Clear" },
    "action.output": { "zh-CN": "输出", en: "Export" },
    "action.cancel": { "zh-CN": "取消", en: "Cancel" },
    "action.close": { "zh-CN": "关闭", en: "Close" },
    "action.saveEntry": { "zh-CN": "保存词条", en: "Save entry" },
    "action.saveAnnotation": { "zh-CN": "保存标注", en: "Save annotation" },
    "action.saveAll": { "zh-CN": "保存全部", en: "Save all" },
    "action.restore": { "zh-CN": "恢复", en: "Restore" },
    "download.docx": { "zh-CN": "下载 DOCX", en: "Download DOCX" },
    "download.pdf": { "zh-CN": "下载 PDF", en: "Download PDF" },
    "download.inProgress": { "zh-CN": "下载中...", en: "Downloading..." },
    "editor.heading": { "zh-CN": "英文文章", en: "English article" },
    "editor.description": {
      "zh-CN": "支持查找替换、草稿历史，以及选词、选句和段落批注。",
      en: "Find and replace, browse draft history, and annotate words, sentences, or paragraphs.",
    },
    "editor.toolbar": { "zh-CN": "文章编辑工具", en: "Article editing tools" },
    "editor.undo": { "zh-CN": "撤销", en: "Undo" },
    "editor.redo": { "zh-CN": "重做", en: "Redo" },
    "editor.find": { "zh-CN": "查找", en: "Find" },
    "editor.history": { "zh-CN": "历史", en: "History" },
    "editor.cleanup": { "zh-CN": "清理格式", en: "Clean formatting" },
    "editor.normalize": { "zh-CN": "英文规范", en: "Normalize English" },
    "editor.vocabulary": { "zh-CN": "词汇管理", en: "Vocabulary" },
    "editor.findText": { "zh-CN": "查找内容", en: "Find text" },
    "editor.replaceText": { "zh-CN": "替换内容", en: "Replacement text" },
    "editor.replaceWith": { "zh-CN": "替换为", en: "Replace with" },
    "editor.caseSensitive": { "zh-CN": "区分大小写", en: "Match case" },
    "editor.findNext": { "zh-CN": "下一个", en: "Find next" },
    "editor.replace": { "zh-CN": "替换", en: "Replace" },
    "editor.replaceAll": { "zh-CN": "全部替换", en: "Replace all" },
    "editor.closeFind": { "zh-CN": "关闭查找", en: "Close find" },
    "editor.selection": { "zh-CN": "已选择文本", en: "Text selected" },
    "editor.entry": { "zh-CN": "词条", en: "Word entry" },
    "editor.dictionary": { "zh-CN": "词典", en: "Dictionary" },
    "editor.sentenceAnalysis": { "zh-CN": "句子解析", en: "Analyze sentence" },
    "editor.paragraphNote": { "zh-CN": "段落批注", en: "Annotate paragraph" },
    "editor.highlight": { "zh-CN": "高亮", en: "Highlight" },
    "editor.placeholder": { "zh-CN": "在此粘贴英文文章…", en: "Paste an English article here…" },
    "editor.resetHeight": { "zh-CN": "恢复输入框高度", en: "Reset editor height" },
    "editor.waiting": { "zh-CN": "等待编辑", en: "Waiting for edits" },
    "editor.saving": { "zh-CN": "正在保存…", en: "Saving…" },
    "editor.saved": { "zh-CN": "草稿已保存", en: "Draft saved" },
    "editor.wordCount": { "zh-CN": "{count} words", en: "{count} words" },
    "editor.stats": {
      "zh-CN": "{words} 词 · {sentences} 句 · {paragraphs} 段 · {minutes} 分钟",
      en: "{words} words · {sentences} sentences · {paragraphs} paragraphs · {minutes} min",
    },
    "editor.autoSaved": { "zh-CN": "已自动保存 {time}", en: "Autosaved at {time}" },
    "editor.saveFailed": { "zh-CN": "草稿保存失败", en: "Could not save draft" },
    "editor.restored": { "zh-CN": "已恢复草稿", en: "Draft restored" },
    "preview.heading": { "zh-CN": "标注预览", en: "Annotated preview" },
    "preview.description": {
      "zh-CN": "预览会跟随编辑栏同步更新，DOCX / PDF 保持同样排版。",
      en: "The preview updates with the editor; DOCX and PDF use the same layout.",
    },
    "preview.waiting": { "zh-CN": "等待文章", en: "Waiting for an article" },
    "preview.toolbar": { "zh-CN": "预览缩放工具", en: "Preview zoom controls" },
    "preview.zoomOut": { "zh-CN": "缩小预览", en: "Zoom out" },
    "preview.zoomIn": { "zh-CN": "放大预览", en: "Zoom in" },
    "preview.resetZoom": { "zh-CN": "恢复 100%", en: "Reset to 100%" },
    "preview.fit": { "zh-CN": "适合宽度", en: "Fit to width" },
    "preview.currentZoom": {
      "zh-CN": "当前预览缩放 {percent}%，点击恢复 100%",
      en: "Preview zoom is {percent}%; click to reset to 100%",
    },
    "preview.focus": { "zh-CN": "放大查看", en: "Focus view" },
    "preview.exitFocus": { "zh-CN": "退出放大", en: "Exit focus view" },
    "preview.empty": {
      "zh-CN": "粘贴英文文章后自动生成预览",
      en: "Paste an English article to generate a preview automatically",
    },
    "preview.ready": { "zh-CN": "预览已生成", en: "Preview ready" },
    "preview.updating": { "zh-CN": "更新中...", en: "Updating..." },
    "preview.failed": { "zh-CN": "预览失败", en: "Preview failed" },
    "preview.missing": { "zh-CN": "{count} 个未收录词", en: "{count} words not found" },
    "settings.translation": { "zh-CN": "翻译与注音", en: "Translation & pronunciation" },
    "settings.translationLanguage": { "zh-CN": "翻译语言", en: "Translation language" },
    "settings.pronunciation": { "zh-CN": "注音方式", en: "Pronunciation guide" },
    "settings.uiLanguage": { "zh-CN": "软件语言", en: "Interface language" },
    "settings.editor": { "zh-CN": "编辑栏", en: "Editor layout" },
    "settings.englishSize": { "zh-CN": "英文大小", en: "English text size" },
    "settings.ipaSize": { "zh-CN": "音标大小", en: "Pronunciation size" },
    "settings.definitionSize": { "zh-CN": "释义大小", en: "Definition size" },
    "settings.definitionLanguageSize": {
      "zh-CN": "{language}释义大小",
      en: "{language} definition size",
    },
    "settings.lineHeight": { "zh-CN": "行间距", en: "Line spacing" },
    "settings.wordSpacing": { "zh-CN": "单词间距", en: "Word spacing" },
    "settings.pageSize": { "zh-CN": "页面大小", en: "Page size" },
    "settings.custom": { "zh-CN": "自定义", en: "Custom" },
    "settings.pageWidth": { "zh-CN": "页面宽度 mm", en: "Page width (mm)" },
    "settings.pageHeight": { "zh-CN": "页面高度 mm", en: "Page height (mm)" },
    "settings.title": { "zh-CN": "标题", en: "Title" },
    "settings.grade": { "zh-CN": "学生年级", en: "Student grade" },
    "settings.loadingGrades": { "zh-CN": "正在读取年级配置...", en: "Loading grade profiles..." },
    "settings.customWords": { "zh-CN": "自定义标注词", en: "Custom vocabulary" },
    "settings.annotations": { "zh-CN": "标注记录", en: "Annotations" },
    "settings.noAnnotations": {
      "zh-CN": "选择单词、句子或段落后即可添加标注。",
      en: "Select a word, sentence, or paragraph to add an annotation.",
    },
    "settings.recordUnknown": {
      "zh-CN": "记录词典未收录词（正文不显示占位符）",
      en: "Record words missing from the dictionary (no placeholders in the article)",
    },
    "settings.offlineMode": {
      "zh-CN": "软件界面语言包已内置，可离线即时切换；释义与注音由本地词库生成。",
      en: "Interface language packs are built in for instant offline switching; definitions and pronunciation are generated from the local dictionary.",
    },
    "pronunciation.ipaUs": { "zh-CN": "美式国际音标（IPA）", en: "American IPA" },
    "pronunciation.ipaUk": { "zh-CN": "英式近似国际音标（IPA）", en: "Approximate British IPA" },
    "pronunciation.ipa": { "zh-CN": "国际音标（通用转写）", en: "IPA (general transcription)" },
    "pronunciation.target": {
      "zh-CN": "易读拼音（拉丁转写）",
      en: "Readable Latin respelling",
    },
    "pronunciation.syllable": { "zh-CN": "音节拆分 + 重音", en: "Syllables + stress" },
    "pronunciation.none": { "zh-CN": "不显示注音", en: "Do not show pronunciation" },
    "pronunciation.usOffline": {
      "zh-CN": "美式 IPA 由内置词典生成",
      en: "American IPA is generated by the built-in dictionary",
    },
    "pronunciation.disabled": { "zh-CN": "已关闭注音", en: "pronunciation is hidden" },
    "pronunciation.builtin": {
      "zh-CN": "所选注音方式由内置发音词典生成",
      en: "the selected pronunciation guide is generated locally",
    },
    "pronunciation.updated": {
      "zh-CN": "注音方式已更新，本地预览已刷新",
      en: "Pronunciation updated; the local preview has refreshed",
    },
    "translation.button": { "zh-CN": "更新多语释义", en: "Update definitions" },
    "translation.translating": { "zh-CN": "翻译中...", en: "Translating..." },
    "translation.chineseNote": {
      "zh-CN": "简体中文释义使用内置词典；{pronunciation}。",
      en: "Simplified Chinese definitions use the built-in dictionary; {pronunciation}.",
    },
    "translation.networkNote": {
      "zh-CN": "{language}首次生成需要联网，结果会随草稿缓存；{pronunciation}。",
      en: "{language} needs internet access the first time it is generated and is then cached with the draft; {pronunciation}.",
    },
    "translation.switchedChinese": {
      "zh-CN": "已切换至简体中文内置词典",
      en: "Switched to the built-in Simplified Chinese dictionary",
    },
    "translation.switched": {
      "zh-CN": "已切换至{language}，即将自动翻译",
      en: "Switched to {language}; translation will start automatically",
    },
    "translation.chineseStatus": {
      "zh-CN": "简体中文使用内置词典，无需自动翻译。",
      en: "Simplified Chinese uses the built-in dictionary; no automatic translation is needed.",
    },
    "translation.waitingForArticle": {
      "zh-CN": "输入英文文章后将自动使用内置翻译。",
      en: "Paste an English article to start built-in translation.",
    },
    "translation.cached": {
      "zh-CN": "已使用草稿中缓存的 {count} 条多语释义。",
      en: "Using {count} cached definitions from this draft.",
    },
    "translation.queued": {
      "zh-CN": "等待内置自动翻译…",
      en: "Built-in translation is queued…",
    },
    "translation.translatingStatus": {
      "zh-CN": "正在生成目标语释义…",
      en: "Generating target-language definitions…",
    },
    "translation.failed": { "zh-CN": "内置翻译失败", en: "Built-in translation failed" },
    "translation.completed": {
      "zh-CN": "内置翻译已生成 {count} 条释义。",
      en: "Generated {count} definitions.",
    },
    "dictionary.pronunciationOnly": {
      "zh-CN": "已补全内置音标；当前目标语暂未找到内置释义。",
      en: "Pronunciation was filled in; no built-in definition was found for the current target language.",
    },
    "dictionary.notFound": {
      "zh-CN": "内置词典暂未收录，可手动填写释义。",
      en: "Not found in the built-in dictionary; you can enter a definition manually.",
    },
    "result.generated": { "zh-CN": "文件已生成", en: "File generated" },
    "result.downloadStarted": { "zh-CN": "文件已开始下载。", en: "The download has started." },
    "dictionary.loading": { "zh-CN": "正在查询本地词典…", en: "Searching the local dictionary…" },
    "dictionary.add": { "zh-CN": "加入词汇标注", en: "Add vocabulary annotation" },
    "dictionary.close": { "zh-CN": "关闭词典", en: "Close dictionary" },
    "dictionary.completed": {
      "zh-CN": "已从本地词典补全",
      en: "Filled from the built-in dictionary",
    },
    "dictionary.noDefinition": {
      "zh-CN": "当前目标语暂无释义",
      en: "No definition is available in the current target language",
    },
    "dictionary.failed": { "zh-CN": "词典查询失败", en: "Dictionary lookup failed" },
    "annotation.noNote": { "zh-CN": "无附加备注", en: "No additional notes" },
    "annotation.delete": { "zh-CN": "删除标注", en: "Delete annotation" },
    "annotation.kind.highlight": { "zh-CN": "高亮", en: "Highlight" },
    "annotation.kind.word": { "zh-CN": "生词", en: "New word" },
    "annotation.kind.important": { "zh-CN": "重点词", en: "Key word" },
    "annotation.kind.ignore": { "zh-CN": "忽略词", en: "Ignored word" },
    "annotation.kind.grammar": { "zh-CN": "语法解析", en: "Grammar analysis" },
    "annotation.kind.pattern": { "zh-CN": "句型结构", en: "Sentence pattern" },
    "annotation.kind.complex": { "zh-CN": "长难句", en: "Complex sentence" },
    "annotation.kind.teacher": { "zh-CN": "教师批注", en: "Teacher note" },
    "annotation.kind.tip": { "zh-CN": "学习提示", en: "Study tip" },
    "wordDialog.heading": { "zh-CN": "词汇标注", en: "Vocabulary annotation" },
    "wordDialog.description": {
      "zh-CN": "注音和目标语释义会同步到自定义词表与输出预览。",
      en: "Pronunciation and target-language definitions are synced to custom vocabulary and the export preview.",
    },
    "wordDialog.word": { "zh-CN": "单词", en: "Word" },
    "wordDialog.type": { "zh-CN": "标注类型", en: "Annotation type" },
    "wordDialog.new": { "zh-CN": "生词", en: "New word" },
    "wordDialog.important": { "zh-CN": "重点词", en: "Key word" },
    "wordDialog.ignore": { "zh-CN": "忽略标注", en: "Ignore" },
    "wordDialog.ipa": { "zh-CN": "音标", en: "Pronunciation" },
    "wordDialog.definition": { "zh-CN": "目标语释义", en: "Target-language definition" },
    "wordDialog.definitionPlaceholder": {
      "zh-CN": "当前语境中的释义",
      en: "Meaning in the current context",
    },
    "wordDialog.color": { "zh-CN": "高亮颜色", en: "Highlight color" },
    "wordDialog.note": { "zh-CN": "学习备注", en: "Study notes" },
    "wordDialog.notePlaceholder": {
      "zh-CN": "词形、搭配或易错点",
      en: "Word forms, collocations, or common pitfalls",
    },
    "noteDialog.heading": { "zh-CN": "添加解析", en: "Add analysis" },
    "noteDialog.grammar": { "zh-CN": "语法解析", en: "Grammar analysis" },
    "noteDialog.pattern": { "zh-CN": "句型结构", en: "Sentence pattern" },
    "noteDialog.complex": { "zh-CN": "长难句", en: "Complex sentence" },
    "noteDialog.teacher": { "zh-CN": "教师批注", en: "Teacher note" },
    "noteDialog.tip": { "zh-CN": "学习提示", en: "Study tip" },
    "noteDialog.content": { "zh-CN": "解析或批注", en: "Analysis or note" },
    "noteDialog.placeholder": {
      "zh-CN": "输入语法说明、句型结构或学习建议",
      en: "Enter a grammar explanation, sentence pattern, or study suggestion",
    },
    "history.heading": { "zh-CN": "草稿历史", en: "Draft history" },
    "history.description": {
      "zh-CN": "自动保留最近 20 个版本。",
      en: "The 20 most recent versions are saved automatically.",
    },
    "history.empty": {
      "zh-CN": "尚未生成历史版本。继续编辑后会自动保存。",
      en: "No history yet. A version will be saved as you continue editing.",
    },
    "history.untitled": { "zh-CN": "未命名文章", en: "Untitled article" },
    "history.blank": { "zh-CN": "空白草稿", en: "Blank draft" },
    "vocabulary.heading": { "zh-CN": "批量词汇管理", en: "Vocabulary manager" },
    "vocabulary.description": {
      "zh-CN": "统一编辑生词、重点词和忽略词。",
      en: "Edit new, key, and ignored words in one place.",
    },
    "vocabulary.definition": { "zh-CN": "释义", en: "Definition" },
    "vocabulary.type": { "zh-CN": "类型", en: "Type" },
    "vocabulary.add": { "zh-CN": "＋ 添加词条", en: "＋ Add entry" },
    "color.yellow": { "zh-CN": "黄色", en: "Yellow" },
    "color.orange": { "zh-CN": "橙色", en: "Orange" },
    "color.green": { "zh-CN": "绿色", en: "Green" },
    "color.blue": { "zh-CN": "蓝色", en: "Blue" },
    "color.pink": { "zh-CN": "粉色", en: "Pink" },
  });

  const SUPPLEMENTAL_MESSAGES = Object.freeze({
    "grade.P1.label": { "zh-CN": "小学1年级", en: "Primary Grade 1" },
    "grade.P1.note": {
      "zh-CN": "几乎所有非基础词都会标注。",
      en: "Nearly every word beyond the basic list is annotated.",
    },
    "grade.P2.label": { "zh-CN": "小学2年级", en: "Primary Grade 2" },
    "grade.P2.note": {
      "zh-CN": "常见课堂词少标，长词继续密集标注。",
      en: "Common classroom words are reduced while longer words remain annotated.",
    },
    "grade.P3.label": { "zh-CN": "小学3年级", en: "Primary Grade 3" },
    "grade.P3.note": {
      "zh-CN": "适合初读短篇故事，保留较多辅助。",
      en: "Designed for early short stories with plenty of reading support.",
    },
    "grade.P4.label": { "zh-CN": "小学4年级", en: "Primary Grade 4" },
    "grade.P4.note": {
      "zh-CN": "中等密度标注，兼顾阅读流畅度。",
      en: "Medium-density annotations balance support and reading flow.",
    },
    "grade.P5.label": { "zh-CN": "小学5年级", en: "Primary Grade 5" },
    "grade.P5.note": {
      "zh-CN": "常见叙事词减少标注，突出难点词。",
      en: "Fewer common narrative words are marked so difficult words stand out.",
    },
    "grade.P6.label": { "zh-CN": "小学6年级", en: "Primary Grade 6" },
    "grade.P6.note": {
      "zh-CN": "面向高年级，主要标注较长词和表达。",
      en: "For upper-primary readers, focusing on longer words and expressions.",
    },
    "grade.M1.label": { "zh-CN": "初中1年级", en: "Middle School 1" },
    "grade.M1.note": {
      "zh-CN": "保留文学性难词、派生词和超纲词。",
      en: "Keeps literary, derived, and above-level vocabulary annotations.",
    },
    "grade.M2.label": { "zh-CN": "初中2年级", en: "Middle School 2" },
    "grade.M2.note": {
      "zh-CN": "减少普通词提示，强化生僻词。",
      en: "Reduces ordinary hints and emphasizes uncommon vocabulary.",
    },
    "grade.M3.label": { "zh-CN": "初中3年级", en: "Middle School 3" },
    "grade.M3.note": {
      "zh-CN": "接近精读版，只标重点难词。",
      en: "A close-reading level that marks only important difficult words.",
    },
    "grade.H1.label": { "zh-CN": "高中1年级", en: "High School 1" },
    "grade.H1.note": {
      "zh-CN": "面向高中阅读，只标学术词、文学难词和自定义词。",
      en: "Focuses on academic, literary, and custom vocabulary.",
    },
    "grade.H2.label": { "zh-CN": "高中2年级", en: "High School 2" },
    "grade.H2.note": {
      "zh-CN": "标注更克制，适合高阶精读和考试阅读。",
      en: "More selective annotation for advanced and exam reading.",
    },
    "grade.H3.label": { "zh-CN": "高中3年级", en: "High School 3" },
    "grade.H3.note": {
      "zh-CN": "接近原文阅读，只保留真正影响理解的注释。",
      en: "Near-original reading with only comprehension-critical notes.",
    },
    "grade.summary": {
      "zh-CN": "预估词汇量约 {count} 词。{note}",
      en: "Estimated vocabulary: {count} words. {note}",
    },
    "uiLanguage.loading": {
      "zh-CN": "正在切换至{language}界面…",
      en: "Switching to the {language} interface…",
    },
    "uiLanguage.ready": {
      "zh-CN": "{language}界面已就绪。",
      en: "The {language} interface is ready.",
    },
    "uiLanguage.cached": {
      "zh-CN": "已从内置语言库载入{language}界面。",
      en: "Loaded the {language} interface from the built-in language library.",
    },
    "uiLanguage.failed": {
      "zh-CN": "无法载入{language}界面，已继续使用当前界面。",
      en: "Could not load the {language} interface; the current interface remains active.",
    },
    "uiLanguage.firstUse": {
      "zh-CN": "{language}界面已内置，无需联网。",
      en: "The {language} interface is built in and does not require internet access.",
    },
    "uiLanguage.base": {
      "zh-CN": "全部可选界面语言均已内置，可离线即时切换。",
      en: "Every selectable interface language is built in for instant offline switching.",
    },
    "output.generatedTitle": {
      "zh-CN": "{format} 已生成",
      en: "{format} generated",
    },
    "output.savedDesktop": {
      "zh-CN": "{filename} 已保存到软件目录的 output 文件夹，点击此处打开。",
      en: "{filename} was saved to the app's output folder. Click here to open it.",
    },
    "output.downloadStarted": {
      "zh-CN": "已开始下载 {filename}。",
      en: "The download of {filename} has started.",
    },
    "output.missingWords": {
      "zh-CN": "{destination} 有 {count} 个未收录词保持原文。",
      en: "{destination} {count} words were not found and remain unchanged.",
    },
    "output.generatedSaved": {
      "zh-CN": "{format} 已生成并保存",
      en: "{format} was generated and saved",
    },
    "output.generatedDownloaded": {
      "zh-CN": "{format} 已生成，已开始下载",
      en: "{format} was generated; the download has started",
    },
    "output.generateFailed": {
      "zh-CN": "{format} 生成失败",
      en: "Could not generate {format}",
    },
    "output.failed": {
      "zh-CN": "{format} 输出失败",
      en: "Could not export {format}",
    },
  });

  const EXTRA_TEXT_PAIRS = [
    ["0 词 · 0 句 · 0 段 · 0 分钟", "0 words · 0 sentences · 0 paragraphs · 0 minutes"],
    ["撤销 Ctrl+Z", "Undo Ctrl+Z"],
    ["重做 Ctrl+Y", "Redo Ctrl+Y"],
    ["查找 Ctrl+F", "Find Ctrl+F"],
    ["黄色高亮", "Yellow highlight"],
    ["绿色高亮", "Green highlight"],
    ["蓝色高亮", "Blue highlight"],
    ["粉色高亮", "Pink highlight"],
    ["橙色高亮", "Orange highlight"],
    ["调整英文文章和标注预览的宽度", "Resize the article and preview panes"],
    ["调整标注预览和编辑栏的宽度", "Resize the preview and settings panes"],
    ["例如：Lesson 37 The Tea Rose", "For example: Lesson 37 The Tea Rose"],
    ["glittered stood\nglittered=ˈɡlɪt.ərd=当前语境释义", "glittered stood\nglittered=ˈɡlɪt.ərd=meaning in context"],
    ["例如：ˈɡlɪt.ərd", "For example: ˈɡlɪt.ərd"],
    ["关闭", "Close"],
    ["删除标注", "Delete annotation"],
    ["删除词条", "Delete entry"],
    ["删除历史版本", "Delete history version"],
    ["无附加备注", "No additional notes"],
    ["尚未生成历史版本。继续编辑后会自动保存。", "No history yet. A version will be saved as you continue editing."],
    ["未命名文章", "Untitled article"],
    ["空白草稿", "Blank draft"],
    ["请先粘贴英文文章", "Paste an English article first"],
    ["已清空", "Cleared"],
    ["已插入演示文本", "Demo text inserted"],
    ["已恢复默认栏宽", "Default pane widths restored"],
    ["已恢复输入框高度", "Editor height restored"],
    ["页面初始化失败", "Page initialization failed"],
    ["编辑工具加载失败", "Editor tools failed to load"],
    ["简体中文", "Simplified Chinese"],
    ["忽略", "Ignore"],
    ["未找到", "No matches"],
    ["当前文本无需调整", "No changes are needed"],
    ["草稿保存失败", "Could not save the draft"],
    ["已恢复草稿", "Draft restored"],
    ["历史版本已恢复", "History version restored"],
    ["格式清理", "Format cleanup"],
    ["生成中...", "Generating..."],
    ["查询中...", "Searching..."],
    ["本地词典暂未收录，可加入词汇标注后手动填写。", "Not found in the local dictionary. Add a vocabulary annotation to enter it manually."],
    ["词典中暂无中文释义", "No Chinese definition is available in the dictionary"],
    ["词典查询失败", "Dictionary lookup failed"],
    ["内置翻译暂未返回可用释义，请检查网络后重试。", "No definitions were returned. Check your internet connection and try again."],
    ["内置翻译请求超时，请检查网络后重试。", "Translation timed out. Check your internet connection and try again."],
    ["所选语言暂不受内置翻译支持。", "The selected language is not currently supported by built-in translation."],
    ["请先选择一个英文单词", "Select an English word first"],
    ["请先选择需要高亮的内容", "Select text to highlight first"],
    ["已添加高亮", "Highlight added"],
    ["已设为忽略词", "Marked as ignored"],
    ["词汇标注已保存", "Vocabulary annotation saved"],
    ["解析或批注已保存", "Analysis or note saved"],
    ["添加段落批注", "Add paragraph note"],
    ["添加句子解析", "Add sentence analysis"],
  ];

  const ATTRIBUTE_PAIRS = [
    ["文章编辑工具", "Article editing tools"],
    ["关闭查找", "Close find"],
    ["预览缩放工具", "Preview zoom controls"],
    ["缩小预览", "Zoom out"],
    ["放大预览", "Zoom in"],
    ["恢复 100%", "Reset to 100%"],
    ["调整英文文章和标注预览的宽度", "Resize the article and preview panes"],
    ["调整标注预览和编辑栏的宽度", "Resize the preview and settings panes"],
    ["关闭词典", "Close dictionary"],
    ["关闭", "Close"],
    ["黄色高亮", "Yellow highlight"],
    ["绿色高亮", "Green highlight"],
    ["蓝色高亮", "Blue highlight"],
    ["粉色高亮", "Pink highlight"],
    ["橙色高亮", "Orange highlight"],
    ["查找内容", "Find text"],
    ["替换为", "Replace with"],
    ["Paste English article here...", "Paste an English article here…"],
    ["例如：Lesson 37 The Tea Rose", "For example: Lesson 37 The Tea Rose"],
    ["例如：ˈɡlɪt.ərd", "For example: ˈɡlɪt.ərd"],
    ["当前语境中的释义", "Meaning in the current context"],
    ["词形、搭配或易错点", "Word forms, collocations, or common pitfalls"],
    ["输入语法说明、句型结构或学习建议", "Enter a grammar explanation, sentence pattern, or study suggestion"],
    ["glittered stood\nglittered=ˈɡlɪt.ərd=当前语境释义", "glittered stood\nglittered=ˈɡlɪt.ərd=meaning in context"],
  ];

  const LANGUAGE_GROUPS = Object.freeze({
    "常用语言": "Common languages",
    "东亚、东南亚与太平洋": "East Asia, Southeast Asia & Pacific",
    "南亚与中亚": "South & Central Asia",
    "欧洲": "Europe",
    "西亚与高加索": "West Asia & Caucasus",
    "美洲与加勒比": "Americas & Caribbean",
    "非洲": "Africa",
  });

  const LANGUAGE_OPTIONS = Object.freeze({
    "zh-Hans": ["中文（简体）", "中文（简体） · Chinese (Simplified)"],
    "zh-Hant": ["中文（繁體）", "中文（繁體） · Chinese (Traditional)"],
    en: ["English · 英语", "English"],
    ja: ["日本語 · 日语", "日本語 · Japanese"],
    ko: ["한국어 · 韩语", "한국어 · Korean"],
    es: ["Español · 西班牙语", "Español · Spanish"],
    fr: ["Français · 法语", "Français · French"],
    de: ["Deutsch · 德语", "Deutsch · German"],
    "pt-BR": ["Português (Brasil) · 巴西葡萄牙语", "Português (Brasil) · Brazilian Portuguese"],
    ru: ["Русский · 俄语", "Русский · Russian"],
    ar: ["العربية · 阿拉伯语", "العربية · Arabic"],
    hi: ["हिन्दी · 印地语", "हिन्दी · Hindi"],
    vi: ["Tiếng Việt · 越南语", "Tiếng Việt · Vietnamese"],
    th: ["ไทย · 泰语", "ไทย · Thai"],
    id: ["Bahasa Indonesia · 印度尼西亚语", "Bahasa Indonesia · Indonesian"],
    ms: ["Bahasa Melayu · 马来语", "Bahasa Melayu · Malay"],
    fil: ["Filipino · 菲律宾语", "Filipino"],
    my: ["မြန်မာ · 缅甸语", "မြန်မာ · Burmese"],
    km: ["ខ្មែរ · 高棉语", "ខ្មែរ · Khmer"],
    lo: ["ລາວ · 老挝语", "ລາວ · Lao"],
    mn: ["Монгол · 蒙古语", "Монгол · Mongolian"],
    mi: ["Māori · 毛利语", "Māori"],
    jv: ["Basa Jawa · 爪哇语", "Basa Jawa · Javanese"],
    su: ["Basa Sunda · 巽他语", "Basa Sunda · Sundanese"],
    ceb: ["Cebuano · 宿务语", "Cebuano"],
    bo: ["བོད་སྐད་ · 藏语", "བོད་སྐད་ · Tibetan"],
    ug: ["ئۇيغۇرچە · 维吾尔语", "ئۇيغۇرچە · Uyghur"],
    bn: ["বাংলা · 孟加拉语", "বাংলা · Bengali"],
    ur: ["اردو · 乌尔都语", "اردو · Urdu"],
    pa: ["ਪੰਜਾਬੀ · 旁遮普语", "ਪੰਜਾਬੀ · Punjabi"],
    ta: ["தமிழ் · 泰米尔语", "தமிழ் · Tamil"],
    te: ["తెలుగు · 泰卢固语", "తెలుగు · Telugu"],
    mr: ["मराठी · 马拉地语", "मराठी · Marathi"],
    gu: ["ગુજરાતી · 古吉拉特语", "ગુજરાતી · Gujarati"],
    kn: ["ಕನ್ನಡ · 卡纳达语", "ಕನ್ನಡ · Kannada"],
    ml: ["മലയാളം · 马拉雅拉姆语", "മലയാളം · Malayalam"],
    ne: ["नेपाली · 尼泊尔语", "नेपाली · Nepali"],
    si: ["සිංහල · 僧伽罗语", "සිංහල · Sinhala"],
    uz: ["Oʻzbek · 乌兹别克语", "Oʻzbek · Uzbek"],
    kk: ["Қазақ · 哈萨克语", "Қазақ · Kazakh"],
    ps: ["پښتو · 普什图语", "پښتو · Pashto"],
    sd: ["سنڌي · 信德语", "سنڌي · Sindhi"],
    ky: ["Кыргызча · 吉尔吉斯语", "Кыргызча · Kyrgyz"],
    tg: ["Тоҷикӣ · 塔吉克语", "Тоҷикӣ · Tajik"],
    tk: ["Türkmençe · 土库曼语", "Türkmençe · Turkmen"],
    it: ["Italiano · 意大利语", "Italiano · Italian"],
    "pt-PT": ["Português (Portugal) · 葡萄牙语", "Português (Portugal) · Portuguese"],
    nl: ["Nederlands · 荷兰语", "Nederlands · Dutch"],
    pl: ["Polski · 波兰语", "Polski · Polish"],
    tr: ["Türkçe · 土耳其语", "Türkçe · Turkish"],
    uk: ["Українська · 乌克兰语", "Українська · Ukrainian"],
    cs: ["Čeština · 捷克语", "Čeština · Czech"],
    ro: ["Română · 罗马尼亚语", "Română · Romanian"],
    hu: ["Magyar · 匈牙利语", "Magyar · Hungarian"],
    el: ["Ελληνικά · 希腊语", "Ελληνικά · Greek"],
    sv: ["Svenska · 瑞典语", "Svenska · Swedish"],
    da: ["Dansk · 丹麦语", "Dansk · Danish"],
    no: ["Norsk · 挪威语", "Norsk · Norwegian"],
    fi: ["Suomi · 芬兰语", "Suomi · Finnish"],
    sk: ["Slovenčina · 斯洛伐克语", "Slovenčina · Slovak"],
    sl: ["Slovenščina · 斯洛文尼亚语", "Slovenščina · Slovenian"],
    hr: ["Hrvatski · 克罗地亚语", "Hrvatski · Croatian"],
    sr: ["Српски · 塞尔维亚语", "Српски · Serbian"],
    bg: ["Български · 保加利亚语", "Български · Bulgarian"],
    lt: ["Lietuvių · 立陶宛语", "Lietuvių · Lithuanian"],
    lv: ["Latviešu · 拉脱维亚语", "Latviešu · Latvian"],
    et: ["Eesti · 爱沙尼亚语", "Eesti · Estonian"],
    ca: ["Català · 加泰罗尼亚语", "Català · Catalan"],
    eu: ["Euskara · 巴斯克语", "Euskara · Basque"],
    gl: ["Galego · 加利西亚语", "Galego · Galician"],
    ga: ["Gaeilge · 爱尔兰语", "Gaeilge · Irish"],
    cy: ["Cymraeg · 威尔士语", "Cymraeg · Welsh"],
    is: ["Íslenska · 冰岛语", "Íslenska · Icelandic"],
    sq: ["Shqip · 阿尔巴尼亚语", "Shqip · Albanian"],
    mk: ["Македонски · 马其顿语", "Македонски · Macedonian"],
    be: ["Беларуская · 白俄罗斯语", "Беларуская · Belarusian"],
    mt: ["Malti · 马耳他语", "Malti · Maltese"],
    lb: ["Lëtzebuergesch · 卢森堡语", "Lëtzebuergesch · Luxembourgish"],
    fa: ["فارسی · 波斯语", "فارسی · Persian"],
    he: ["עברית · 希伯来语", "עברית · Hebrew"],
    hy: ["Հայերեն · 亚美尼亚语", "Հայերեն · Armenian"],
    ka: ["ქართული · 格鲁吉亚语", "ქართული · Georgian"],
    az: ["Azərbaycanca · 阿塞拜疆语", "Azərbaycanca · Azerbaijani"],
    ku: ["Kurdî · 库尔德语", "Kurdî · Kurdish"],
    ht: ["Kreyòl ayisyen · 海地克里奥尔语", "Kreyòl ayisyen · Haitian Creole"],
    sw: ["Kiswahili · 斯瓦希里语", "Kiswahili · Swahili"],
    af: ["Afrikaans · 南非语", "Afrikaans"],
    am: ["አማርኛ · 阿姆哈拉语", "አማርኛ · Amharic"],
    so: ["Soomaali · 索马里语", "Soomaali"],
    ha: ["Hausa · 豪萨语", "Hausa"],
    yo: ["Yorùbá · 约鲁巴语", "Yorùbá"],
    zu: ["isiZulu · 祖鲁语", "isiZulu · Zulu"],
    ig: ["Igbo · 伊博语", "Igbo"],
    om: ["Afaan Oromoo · 奥罗莫语", "Afaan Oromoo · Oromo"],
    xh: ["isiXhosa · 科萨语", "isiXhosa · Xhosa"],
    rw: ["Ikinyarwanda · 卢旺达语", "Ikinyarwanda · Kinyarwanda"],
    mg: ["Malagasy · 马达加斯加语", "Malagasy"],
    ny: ["Chichewa · 齐切瓦语", "Chichewa"],
  });

  const ALL_LOCALES = Object.freeze(Object.keys(LANGUAGE_OPTIONS));
  const englishCatalog = Object.create(null);
  const chineseCatalog = Object.create(null);
  const sourceKeyByEnglish = new Map();
  const sourceKeyByChinese = new Map();

  function registerCatalogEntry(key, chinese, english) {
    const normalizedChinese = String(chinese == null ? "" : chinese);
    const normalizedEnglish = String(english == null ? normalizedChinese : english);
    chineseCatalog[key] = normalizedChinese;
    englishCatalog[key] = normalizedEnglish;
    if (normalizedChinese && !sourceKeyByChinese.has(normalizedChinese)) {
      sourceKeyByChinese.set(normalizedChinese, key);
    }
    if (normalizedEnglish && !sourceKeyByEnglish.has(normalizedEnglish)) {
      sourceKeyByEnglish.set(normalizedEnglish, key);
    }
  }

  [MESSAGES, SUPPLEMENTAL_MESSAGES].forEach((messages) => {
    Object.keys(messages).forEach((key) => {
      const message = messages[key];
      registerCatalogEntry(key, message["zh-CN"], message.en);
    });
  });
  EXTRA_TEXT_PAIRS.forEach(([chinese, english], index) => {
    registerCatalogEntry(`legacy.text.${index}`, chinese, english);
  });
  ATTRIBUTE_PAIRS.forEach(([chinese, english], index) => {
    registerCatalogEntry(`legacy.attribute.${index}`, chinese, english);
  });
  Object.entries(LANGUAGE_GROUPS).forEach(([chinese, english], index) => {
    registerCatalogEntry(`language.group.${index}`, chinese, english);
  });

  Object.freeze(englishCatalog);
  Object.freeze(chineseCatalog);

  function catalogHash(catalog) {
    let hash = 2166136261;
    const serialized = JSON.stringify(catalog);
    for (let index = 0; index < serialized.length; index += 1) {
      hash ^= serialized.charCodeAt(index);
      hash = Math.imul(hash, 16777619);
    }
    return (hash >>> 0).toString(36);
  }

  const CATALOG_VERSION = `2-${catalogHash(englishCatalog)}`;
  const bundledLanguagePacks =
    global.YujieUiLanguagePacks &&
    String(global.YujieUiLanguagePacks.version || "") === CATALOG_VERSION &&
    global.YujieUiLanguagePacks.packs &&
    typeof global.YujieUiLanguagePacks.packs === "object"
      ? global.YujieUiLanguagePacks.packs
      : {};
  const SUPPORTED_LOCALES = Object.freeze([
    "zh-Hans",
    "en",
    ...ALL_LOCALES.filter(
      (locale) =>
        !BUILT_IN_LOCALES.has(locale) &&
        Object.prototype.hasOwnProperty.call(bundledLanguagePacks, locale),
    ),
  ]);
  const localeByLowerCase = new Map(
    SUPPORTED_LOCALES.map((locale) => [locale.toLowerCase(), locale]),
  );
  const supportedLocaleSet = new Set(SUPPORTED_LOCALES);
  const catalogs = new Map([
    ["zh-Hans", chineseCatalog],
    ["en", englishCatalog],
  ]);
  const trackedTextNodes = new WeakMap();
  const trackedAttributes = new WeakMap();
  const boundLocaleControls = new WeakSet();
  let localeRequestId = 0;

  function normalizeLocale(locale) {
    const normalized = String(locale || "").trim().replace(/_/g, "-").toLowerCase();
    if (!normalized) return DEFAULT_LOCALE;
    if (normalized === "zh-cn" || normalized === "zh-sg" || normalized === "zh-hans-cn") {
      return "zh-Hans";
    }
    if (
      normalized === "zh-tw" ||
      normalized === "zh-hk" ||
      normalized === "zh-mo" ||
      normalized.startsWith("zh-hant")
    ) {
      return localeByLowerCase.get("zh-hant") || DEFAULT_LOCALE;
    }
    if (normalized === "en" || normalized.startsWith("en-")) return "en";
    return localeByLowerCase.get(normalized) || DEFAULT_LOCALE;
  }

  function readStoredLocale() {
    try {
      return normalizeLocale(global.localStorage.getItem(STORAGE_KEY));
    } catch (_error) {
      return DEFAULT_LOCALE;
    }
  }

  let preferredLocale = readStoredLocale();
  let currentLocale = BUILT_IN_LOCALES.has(preferredLocale) ? preferredLocale : "en";

  function placeholderSignature(value) {
    return (String(value).match(/\{[A-Za-z][A-Za-z0-9_.-]*\}/g) || []).sort().join("\n");
  }

  function maskPlaceholders(value) {
    let index = 0;
    return String(value).replace(/\{[A-Za-z][A-Za-z0-9_.-]*\}/g, () => {
      const token = `__YJ_PH_${index}__`;
      index += 1;
      return token;
    });
  }

  function restorePlaceholders(source, translated) {
    const placeholders = String(source).match(/\{[A-Za-z][A-Za-z0-9_.-]*\}/g) || [];
    let restored = String(translated);
    placeholders.forEach((placeholder, index) => {
      restored = restored.split(`__YJ_PH_${index}__`).join(placeholder);
    });
    return restored;
  }

  function transportCatalog() {
    const catalog = {};
    Object.keys(englishCatalog).forEach((key) => {
      catalog[key] = maskPlaceholders(englishCatalog[key]);
    });
    return catalog;
  }

  function sanitizeRemoteCatalog(value) {
    let raw = value;
    if (typeof raw === "string") {
      try {
        raw = JSON.parse(raw);
      } catch (_error) {
        return {};
      }
    }
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};

    const sanitized = {};
    Object.keys(englishCatalog).forEach((key) => {
      if (typeof raw[key] !== "string") return;
      const source = englishCatalog[key];
      const translated = restorePlaceholders(source, raw[key]).trim();
      const maximumLength = Math.max(512, source.length * 12 + 128);
      if (
        !translated ||
        translated.length > maximumLength ||
        placeholderSignature(translated) !== placeholderSignature(source)
      ) {
        return;
      }
      sanitized[key] = translated;
    });
    return Object.keys(sanitized).length === Object.keys(englishCatalog).length
      ? sanitized
      : {};
  }

  function bundledCatalog(locale) {
    const targetLocale = normalizeLocale(locale);
    const bundle = global.YujieUiLanguagePacks;
    if (
      !bundle ||
      String(bundle.version || "") !== CATALOG_VERSION ||
      !bundle.packs ||
      typeof bundle.packs !== "object"
    ) {
      return null;
    }
    const rawPack = bundle.packs[targetLocale];
    if (!rawPack) return null;
    const pack = sanitizeRemoteCatalog(rawPack);
    return Object.keys(pack).length ? pack : null;
  }

  const initialBundledCatalog = bundledCatalog(preferredLocale);
  if (initialBundledCatalog) {
    catalogs.set(preferredLocale, initialBundledCatalog);
    currentLocale = preferredLocale;
  }

  async function ensureCatalog(locale) {
    const targetLocale = normalizeLocale(locale);
    if (catalogs.has(targetLocale)) return catalogs.get(targetLocale);

    const bundled = bundledCatalog(targetLocale);
    if (bundled) {
      catalogs.set(targetLocale, bundled);
      return bundled;
    }
    throw new Error(`The ${targetLocale} interface is not embedded in this build.`);
  }

  function interpolate(value, variables) {
    if (!variables) return value;
    return String(value).replace(/\{([^{}]+)\}/g, (match, name) =>
      Object.prototype.hasOwnProperty.call(variables, name) ? String(variables[name]) : match,
    );
  }

  function getLocale() {
    return currentLocale;
  }

  function isBuiltInLocale(locale) {
    return BUILT_IN_LOCALES.has(normalizeLocale(locale));
  }

  function isLocaleLoaded(locale) {
    const targetLocale = normalizeLocale(locale);
    return catalogs.has(targetLocale) || Boolean(bundledCatalog(targetLocale));
  }

  function hasCachedLocale(_locale) {
    return false;
  }

  function exportCatalog(options) {
    const masked = !options || options.masked !== false;
    return { ...(masked ? transportCatalog() : englishCatalog) };
  }

  function resolveCatalogKey(key) {
    const direct = String(key);
    if (Object.prototype.hasOwnProperty.call(englishCatalog, direct)) return direct;
    return sourceKeyByChinese.get(direct) || sourceKeyByEnglish.get(direct) || null;
  }

  function resolveMessage(key, locale) {
    const catalogKey = resolveCatalogKey(key);
    if (!catalogKey) return String(key);
    const targetLocale = normalizeLocale(locale);
    if (targetLocale === "zh-Hans") {
      return chineseCatalog[catalogKey] || englishCatalog[catalogKey] || String(key);
    }
    const targetCatalog = catalogs.get(targetLocale);
    return (
      targetCatalog?.[catalogKey] ||
      englishCatalog[catalogKey] ||
      chineseCatalog[catalogKey] ||
      String(key)
    );
  }

  function t(key, variables, locale) {
    const targetLocale = normalizeLocale(locale || currentLocale);
    return interpolate(resolveMessage(key, targetLocale), variables);
  }

  function replaceTrimmedText(node, locale) {
    const value = node.nodeValue || "";
    const trimmed = value.trim();
    if (!trimmed) return;

    let source = trackedTextNodes.get(node);
    if (!source) {
      const key = resolveCatalogKey(trimmed);
      if (!key) return;
      source = { key };
      trackedTextNodes.set(node, source);
    }

    const replacement = resolveMessage(source.key, locale);
    const start = value.indexOf(trimmed);
    node.nodeValue = `${value.slice(0, start)}${replacement}${value.slice(start + trimmed.length)}`;
  }

  function shouldSkipTextNode(node) {
    const parent = node.parentElement;
    if (!parent) return true;
    if (parent.closest("[data-i18n-skip], [contenteditable='true']")) return true;
    return ["SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE"].includes(parent.tagName);
  }

  function applyText(root, locale) {
    const doc = root.nodeType === 9 ? root : root.ownerDocument;
    if (!doc || !doc.createTreeWalker) return;
    const showText = global.NodeFilter ? global.NodeFilter.SHOW_TEXT : 4;
    const walker = doc.createTreeWalker(root, showText);
    let node = walker.nextNode();
    while (node) {
      if (!shouldSkipTextNode(node)) replaceTrimmedText(node, locale);
      node = walker.nextNode();
    }
  }

  function applyAttributes(root, locale) {
    const elements = [];
    if (root.nodeType === 1 && root.matches("[placeholder], [title], [aria-label]")) {
      elements.push(root);
    }
    if (root.querySelectorAll) {
      elements.push(...root.querySelectorAll("[placeholder], [title], [aria-label]"));
    }

    elements.forEach((element) => {
      let tracked = trackedAttributes.get(element);
      if (!tracked) {
        tracked = {};
        trackedAttributes.set(element, tracked);
      }
      ["placeholder", "title", "aria-label"].forEach((attribute) => {
        if (!element.hasAttribute(attribute)) return;
        const current = element.getAttribute(attribute);
        if (!tracked[attribute]) {
          const key = resolveCatalogKey(current);
          if (key) tracked[attribute] = { key };
        }
        const source = tracked[attribute];
        if (source) element.setAttribute(attribute, resolveMessage(source.key, locale));
      });
    });
  }

  function applyLanguageOptions(root, locale) {
    const select =
      root.nodeType === 1 && root.id === "translationLanguage"
        ? root
        : root.querySelector && root.querySelector("#translationLanguage");
    if (!select) return;

    select.querySelectorAll("optgroup").forEach((group) => {
      let key = group.dataset.i18nGroupKey;
      if (!key) {
        const current = group.getAttribute("label") || "";
        const chinese = Object.prototype.hasOwnProperty.call(LANGUAGE_GROUPS, current)
          ? current
          : Object.keys(LANGUAGE_GROUPS).find(
              (candidate) => LANGUAGE_GROUPS[candidate] === current,
            );
        key = chinese ? resolveCatalogKey(chinese) : "";
        if (key) group.dataset.i18nGroupKey = key;
      }
      if (key) group.setAttribute("label", resolveMessage(key, locale));
    });

    select.querySelectorAll("option").forEach((option) => {
      const labels = LANGUAGE_OPTIONS[option.value];
      if (labels) option.textContent = locale === "zh-Hans" ? labels[0] : labels[1];
    });
  }

  function nativeLocaleName(locale) {
    const labels = LANGUAGE_OPTIONS[locale];
    if (!labels) return locale;
    return labels[0].split("·")[0].trim();
  }

  function populateLocaleControls(root, locale) {
    if (!root.querySelectorAll) return;
    const documentRoot = root.nodeType === 9 ? root : root.ownerDocument;
    const sourceSelect = documentRoot?.querySelector("#translationLanguage");
    const controls = [];
    if (
      root.nodeType === 1 &&
      root.matches("#uiLanguage, #uiLocale, #softwareLanguage, [data-ui-locale]")
    ) {
      controls.push(root);
    }
    controls.push(
      ...root.querySelectorAll(
        "#uiLanguage, #uiLocale, #softwareLanguage, [data-ui-locale]",
      ),
    );

    controls.forEach((control) => {
      if (control.tagName !== "SELECT") {
        return;
      }
      if (control.dataset.uiCatalogVersion === CATALOG_VERSION) {
        control.value = currentLocale;
        return;
      }

      const fragment = documentRoot.createDocumentFragment();
      const included = new Set();
      if (sourceSelect) {
        Array.from(sourceSelect.children).forEach((sourceGroup) => {
          if (sourceGroup.tagName !== "OPTGROUP") return;
          const group = documentRoot.createElement("optgroup");
          const chineseLabel = sourceGroup.getAttribute("label") || "";
          const groupKey =
            sourceGroup.dataset.i18nGroupKey || resolveCatalogKey(chineseLabel) || "";
          if (groupKey) group.dataset.i18nGroupKey = groupKey;
          group.label = groupKey
            ? resolveMessage(groupKey, locale)
            : locale === "zh-Hans"
              ? chineseLabel
              : LANGUAGE_GROUPS[chineseLabel] || chineseLabel;
          Array.from(sourceGroup.children).forEach((sourceOption) => {
            const code = sourceOption.value;
            if (!LANGUAGE_OPTIONS[code] || !supportedLocaleSet.has(code)) return;
            const option = documentRoot.createElement("option");
            option.value = code;
            option.textContent = nativeLocaleName(code);
            group.appendChild(option);
            included.add(code);
          });
          if (group.children.length) fragment.appendChild(group);
        });
      }
      if (included.size !== SUPPORTED_LOCALES.length) {
        const group = documentRoot.createElement("optgroup");
        group.label = locale === "zh-Hans" ? "其他语言" : "Other languages";
        SUPPORTED_LOCALES.forEach((code) => {
          if (included.has(code)) return;
          const option = documentRoot.createElement("option");
          option.value = code;
          option.textContent = nativeLocaleName(code);
          group.appendChild(option);
        });
        fragment.appendChild(group);
      }
      control.replaceChildren(fragment);
      control.dataset.uiCatalogVersion = CATALOG_VERSION;
      control.value = currentLocale;
    });
  }

  function bindLocaleControls(root) {
    if (!root.querySelectorAll) return;
    const selector = "#uiLanguage, #uiLocale, #softwareLanguage, [data-ui-locale]";
    const controls = [];
    if (root.nodeType === 1 && root.matches(selector)) controls.push(root);
    controls.push(...root.querySelectorAll(selector));
    controls.forEach((control) => {
      control.value = currentLocale;
      if (boundLocaleControls.has(control)) return;
      control.addEventListener("change", () => {
        setLocale(control.value)
          .catch(() => {
            applyDocument(global.document, currentLocale);
          })
          .finally(() => {
            // A language selector must never remain locked, even when another
            // locale-dependent refresh throws in a host-specific WebView.
            control.disabled = false;
            control.value = currentLocale;
          });
      });
      boundLocaleControls.add(control);
    });
  }

  function applyDocument(root, locale) {
    const target = root && typeof root !== "string" ? root : global.document;
    const targetLocale = normalizeLocale(
      typeof root === "string" ? root : locale || currentLocale,
    );
    if (!target) return targetLocale;
    const doc = target.nodeType === 9 ? target : target.ownerDocument;

    if (doc && (target.nodeType === 9 || target === doc.documentElement || target === doc.body)) {
      doc.documentElement.lang = targetLocale;
      doc.documentElement.dir = RTL_LOCALES.has(targetLocale) ? "rtl" : "ltr";
      doc.title = t("app.name", null, targetLocale);
      const description = doc.querySelector('meta[name="description"]');
      if (description) {
        description.setAttribute("content", t("app.description", null, targetLocale));
      }
    }

    populateLocaleControls(target, targetLocale);
    applyLanguageOptions(target, targetLocale);
    applyText(target, targetLocale);
    applyAttributes(target, targetLocale);
    bindLocaleControls(target);
    return targetLocale;
  }

  function persistLocale(locale) {
    try {
      global.localStorage.setItem(STORAGE_KEY, locale);
    } catch (_error) {
      // The active interface still works when storage is unavailable.
    }
  }

  function dispatchLocaleChange(requestedLocale, loaded, error) {
    if (!global.document || typeof global.CustomEvent !== "function") return;
    global.document.dispatchEvent(
      new global.CustomEvent("yujie:localechange", {
        detail: {
          locale: currentLocale,
          requestedLocale,
          loaded,
          fallbackLocale: loaded ? null : currentLocale,
          error: error ? String(error.message || error) : null,
        },
      }),
    );
  }

  async function setLocale(locale, options) {
    const nextLocale = normalizeLocale(locale);
    const previousLocale = currentLocale;
    const requestId = ++localeRequestId;

    if (global.document?.documentElement) {
      global.document.documentElement.dataset.i18nLoading =
        catalogs.has(nextLocale) ? "false" : "true";
    }
    try {
      await ensureCatalog(nextLocale);
      if (requestId !== localeRequestId) return currentLocale;
      currentLocale = nextLocale;
      preferredLocale = nextLocale;
      persistLocale(nextLocale);
      if (global.document?.documentElement) {
        global.document.documentElement.dataset.i18nLoading = "false";
      }
      if (!options || options.apply !== false) applyDocument(global.document, nextLocale);
      dispatchLocaleChange(nextLocale, true, null);
      return nextLocale;
    } catch (error) {
      if (requestId !== localeRequestId) return currentLocale;
      currentLocale = previousLocale;
      preferredLocale = previousLocale;
      persistLocale(previousLocale);
      if (global.document?.documentElement) {
        global.document.documentElement.dataset.i18nLoading = "false";
      }
      if (!options || options.apply !== false) applyDocument(global.document, previousLocale);
      dispatchLocaleChange(nextLocale, false, error);
      return previousLocale;
    }
  }

  function formatTime(value, options) {
    const date = value instanceof Date ? value : new Date(value == null ? Date.now() : value);
    if (Number.isNaN(date.getTime())) return "";
    const formatOptions = options || { hour: "2-digit", minute: "2-digit" };
    try {
      return new Intl.DateTimeFormat(currentLocale, formatOptions).format(date);
    } catch (_error) {
      return date.toLocaleTimeString();
    }
  }

  let readyPromise = null;
  function ready() {
    if (!readyPromise) {
      const localeAtStart = preferredLocale;
      const requestIdAtStart = localeRequestId;
      if (localeAtStart === currentLocale && catalogs.has(localeAtStart)) {
        readyPromise = Promise.resolve(currentLocale);
      } else {
        if (global.document?.documentElement) {
          global.document.documentElement.dataset.i18nLoading = "true";
        }
        readyPromise = ensureCatalog(localeAtStart)
          .then(() => {
            if (requestIdAtStart !== localeRequestId) return currentLocale;
            currentLocale = localeAtStart;
            if (global.document?.documentElement) {
              global.document.documentElement.dataset.i18nLoading = "false";
            }
            applyDocument(global.document, localeAtStart);
            dispatchLocaleChange(localeAtStart, true, null);
            return currentLocale;
          })
          .catch((error) => {
            if (requestIdAtStart !== localeRequestId) return currentLocale;
            preferredLocale = currentLocale;
            persistLocale(currentLocale);
            if (global.document?.documentElement) {
              global.document.documentElement.dataset.i18nLoading = "false";
            }
            applyDocument(global.document, currentLocale);
            dispatchLocaleChange(localeAtStart, false, error);
            return currentLocale;
          });
      }
    }
    return readyPromise;
  }

  global.YujieI18n = Object.freeze({
    STORAGE_KEY,
    CATALOG_VERSION,
    packLocales: ALL_LOCALES,
    supportedLocales: SUPPORTED_LOCALES,
    getLocale,
    isBuiltInLocale,
    isLocaleLoaded,
    hasCachedLocale,
    exportCatalog,
    setLocale,
    t,
    applyDocument,
    formatTime,
    ready,
  });

  if (global.document) {
    if (global.document.readyState === "loading") {
      global.document.addEventListener(
        "DOMContentLoaded",
        () => {
          ready().then(() => applyDocument(global.document));
        },
        { once: true },
      );
    } else {
      ready().then(() => applyDocument(global.document));
    }
  }
})(window);
