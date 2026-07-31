"use client";

import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent,
} from "react";

type Locale = "zh" | "en";
type Localized = { zh: string; en: string };
type DefinitionLanguage = "zh" | "ja" | "es";

const PUBLIC_BASE_PATH = process.env.NEXT_PUBLIC_BASE_PATH ?? "";
const publicAsset = (pathname: string) => `${PUBLIC_BASE_PATH}${pathname}`;

const RELEASE = {
  version: "1.4.1",
  date: "2026年7月30日",
  zipUrl:
    "https://github.com/zkmaojack/cijing-reader/releases/download/v1.4.1/yujie-reader-v1.4.1-portable.zip",
  exeUrl:
    "https://github.com/zkmaojack/cijing-reader/releases/download/v1.4.1/yujie-reader-v1.4.1-windows.exe",
  checksumUrl:
    "https://github.com/zkmaojack/cijing-reader/releases/download/v1.4.1/SHA256SUMS.txt",
  releaseUrl:
    "https://github.com/zkmaojack/cijing-reader/releases/tag/v1.4.1",
  latestUrl: "https://github.com/zkmaojack/cijing-reader/releases/latest",
  zipSize: "8.93 MB",
  exeSize: "25.01 MB",
  zipSha:
    "7B47A2CD6FF62D9FF941CE04F4869FD7ECE1461B37BC0DCF4F7EDBE08A0D8C94",
  exeSha:
    "5ECF6597816A73C08C0DCF78BC58D8BAE498DF2EFA868079950A51A776C1005F",
};

const navItems: Array<{ href: string; label: Localized }> = [
  { href: `${PUBLIC_BASE_PATH}/`, label: { zh: "作品集", en: "Portfolio" } },
  { href: "#demo", label: { zh: "互动演示", en: "Live demo" } },
  { href: "#features", label: { zh: "功能", en: "Features" } },
  { href: "#workflow", label: { zh: "使用方式", en: "Workflow" } },
  { href: "#download", label: { zh: "下载", en: "Download" } },
];

const features: Array<{
  number: string;
  title: Localized;
  body: Localized;
  accent: string;
  metric: string;
}> = [
  {
    number: "01",
    title: { zh: "真正按年级标注", en: "Level-aware vocabulary" },
    body: {
      zh: "从小学一年级到高中三年级，减少已掌握词汇的干扰，只突出真正影响理解的难词。",
      en: "Twelve student profiles keep familiar words quiet and surface the vocabulary that truly blocks understanding.",
    },
    accent: "grade",
    metric: "P1—H3",
  },
  {
    number: "02",
    title: { zh: "多语言，从界面开始", en: "Multilingual by design" },
    body: {
      zh: "98 种界面语言全部内置，近 100 种目标释义语言，让学习者用熟悉的语言进入英文。",
      en: "Switch among 98 built-in interface languages and read definitions in nearly 100 target languages.",
    },
    accent: "world",
    metric: "98",
  },
  {
    number: "03",
    title: { zh: "注音不止一种答案", en: "Pronunciation your way" },
    body: {
      zh: "美式 IPA、英式近似 IPA、通用 IPA、易读转写、音节重音，也可以完全关闭。",
      en: "Choose US, UK-style or general IPA, readable transliteration, syllable stress, or no guide at all.",
    },
    accent: "sound",
    metric: "/ˈwɜːd/",
  },
  {
    number: "04",
    title: { zh: "本地优先，不用配密钥", en: "Local-first essentials" },
    body: {
      zh: "内置英汉词典和发音数据。简体中文释义与默认音标无需联网，也无需 API 配置。",
      en: "Built-in dictionary and pronunciation data provide Chinese definitions and IPA without API setup.",
    },
    accent: "offline",
    metric: "LOCAL",
  },
  {
    number: "05",
    title: { zh: "从单词走到长难句", en: "Beyond word lookup" },
    body: {
      zh: "词典、重点词、多色高亮、语法、句型、长难句解析与教师批注，都在同一篇文章里完成。",
      en: "Combine vocabulary, highlights, grammar, complex-sentence analysis, and teacher notes in one article.",
    },
    accent: "analysis",
    metric: "A → Z",
  },
  {
    number: "06",
    title: { zh: "从草稿直接变讲义", en: "From draft to handout" },
    body: {
      zh: "实时预览与 DOCX、PDF 排版保持一致，并自动保留最近 20 个草稿版本。",
      en: "Keep preview and exported documents aligned, with automatic history for the latest 20 drafts.",
    },
    accent: "export",
    metric: "DOCX · PDF",
  },
];

const workflow: Array<{
  step: string;
  title: Localized;
  body: Localized;
  note: Localized;
}> = [
  {
    step: "01",
    title: { zh: "粘贴文章", en: "Paste an article" },
    body: {
      zh: "导入需要精读的英文内容，用查找替换、格式清理和英文规范工具快速整理文本。",
      en: "Bring in any English passage and clean it up with editing, find-and-replace, and formatting tools.",
    },
    note: { zh: "原文进入", en: "Text in" },
  },
  {
    step: "02",
    title: { zh: "匹配学习者", en: "Match the learner" },
    body: {
      zh: "选择学生年级、目标释义语言与注音方式，让标注密度贴合真实水平。",
      en: "Choose a student level, definition language, and pronunciation style to tune the annotation density.",
    },
    note: { zh: "难度适配", en: "Level fit" },
  },
  {
    step: "03",
    title: { zh: "完善并导出", en: "Refine and export" },
    body: {
      zh: "添加重点词、句子解析和教师批注，在实时预览中确认后导出 DOCX 或 PDF。",
      en: "Add vocabulary, sentence explanations, and teacher notes, then preview and export DOCX or PDF.",
    },
    note: { zh: "讲义完成", en: "Handout out" },
  },
];

const faqs: Array<{ question: Localized; answer: Localized }> = [
  {
    question: { zh: "支持哪些系统？", en: "Which systems are supported?" },
    answer: {
      zh: "目前支持 Windows 10 / 11 的 64 位电脑。暂不支持 macOS、Linux、Windows ARM 或 32 位 Windows。",
      en: "Yujie Reader currently supports 64-bit Windows 10 and Windows 11. macOS, Linux, Windows ARM, and 32-bit Windows are not yet supported.",
    },
  },
  {
    question: { zh: "需要安装或注册吗？", en: "Does it require installation or an account?" },
    answer: {
      zh: "不需要。下载便携 ZIP 后解压，双击“语界精读.exe”即可；也可直接下载单文件 EXE。",
      en: "No. Extract the portable ZIP and run “语界精读.exe”, or download the standalone EXE.",
    },
  },
  {
    question: { zh: "所有功能都可以离线吗？", en: "Does everything work offline?" },
    answer: {
      zh: "不是所有功能。界面切换、分级标注、简体中文释义、IPA 和 DOCX 导出可离线使用；其他目标语言首次生成释义时需要联网。",
      en: "Not every feature. Interface switching, level-based annotation, Chinese definitions, IPA, and DOCX export work offline. Other target languages need a connection when first generated.",
    },
  },
  {
    question: { zh: "需要自己配置 API 密钥吗？", en: "Do I need an API key?" },
    answer: {
      zh: "不需要。软件没有接口地址、模型或密钥配置步骤。",
      en: "No. There are no API endpoints, models, or keys for you to configure.",
    },
  },
  {
    question: { zh: "为什么可能出现 Windows 安全提示？", en: "Why might Windows show a security warning?" },
    answer: {
      zh: "当前 v1.4.1 尚未进行数字签名，Windows 可能显示安全提醒。请只从本站或项目的 GitHub Release 下载，并核对页面提供的 SHA-256。",
      en: "Version 1.4.1 is not yet digitally signed, so Windows may show a warning. Download only from this site or the official GitHub Release and verify the SHA-256.",
    },
  },
  {
    question: { zh: "PDF 无法导出怎么办？", en: "What if PDF export is unavailable?" },
    answer: {
      zh: "请确认电脑已安装 Microsoft Edge 或 Google Chrome；也可以先导出无需浏览器支持的 DOCX。",
      en: "Make sure Microsoft Edge or Google Chrome is installed. You can also export DOCX, which works offline.",
    },
  },
];

const glossary: Record<
  string,
  {
    ipa: string;
    definitions: Record<DefinitionLanguage, string>;
    type: Localized;
  }
> = {
  weathered: {
    ipa: "/ˈweð.əd/",
    definitions: { zh: "饱经风霜的；风化的", ja: "風雨にさらされた", es: "desgastado por el tiempo" },
    type: { zh: "形容词", en: "adjective" },
  },
  delicate: {
    ipa: "/ˈdel.ɪ.kət/",
    definitions: { zh: "精致的；娇嫩的", ja: "繊細な", es: "delicado" },
    type: { zh: "形容词", en: "adjective" },
  },
  amber: {
    ipa: "/ˈæm.bər/",
    definitions: { zh: "琥珀色的", ja: "琥珀色の", es: "ámbar" },
    type: { zh: "形容词", en: "adjective" },
  },
  unfurled: {
    ipa: "/ʌnˈfɜːld/",
    definitions: { zh: "舒展开；展开", ja: "広がった", es: "se desplegó" },
    type: { zh: "动词", en: "verb" },
  },
  lingering: {
    ipa: "/ˈlɪŋ.ɡər.ɪŋ/",
    definitions: { zh: "迟迟不散的；萦绕的", ja: "余韻の残る", es: "persistente" },
    type: { zh: "形容词", en: "adjective" },
  },
  fragrance: {
    ipa: "/ˈfreɪ.ɡrəns/",
    definitions: { zh: "芳香；香气", ja: "香り", es: "fragancia" },
    type: { zh: "名词", en: "noun" },
  },
};

const articleTokens = [
  "The",
  "tea",
  "rose",
  "climbed",
  "over",
  "the",
  "weathered",
  "garden",
  "wall,",
  "its",
  "delicate",
  "petals",
  "catching",
  "the",
  "amber",
  "light.",
  "By",
  "dusk,",
  "each",
  "bud",
  "had",
  "unfurled,",
  "leaving",
  "a",
  "lingering",
  "fragrance",
  "in",
  "the",
  "quiet",
  "air.",
];

const levelWords: Record<number, string[]> = {
  4: ["weathered", "delicate", "amber", "unfurled", "lingering", "fragrance"],
  7: ["weathered", "unfurled", "lingering", "fragrance"],
  10: ["unfurled", "lingering"],
};

function cleanToken(token: string) {
  return token.toLowerCase().replace(/[^a-z]/g, "");
}

export default function HomeClient() {
  const [locale, setLocale] = useState<Locale>("zh");
  const [level, setLevel] = useState(7);
  const [definitionLanguage, setDefinitionLanguage] =
    useState<DefinitionLanguage>("zh");
  const [activeWord, setActiveWord] = useState("lingering");
  const [demoTouched, setDemoTouched] = useState(false);
  const stageRef = useRef<HTMLDivElement>(null);

  const pick = (value: Localized) => value[locale];
  const highlightedWords = useMemo(() => levelWords[level], [level]);
  const activeEntry = glossary[activeWord];

  useEffect(() => {
    const saved = window.localStorage.getItem("yujie-site-locale");
    if (saved === "zh" || saved === "en") setLocale(saved);

    const root = document.documentElement;
    root.classList.add("motion-ready");
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            entry.target.classList.add("is-visible");
            observer.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.14 },
    );
    document.querySelectorAll(".reveal").forEach((element) => observer.observe(element));
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    window.localStorage.setItem("yujie-site-locale", locale);
    document.documentElement.lang = locale === "zh" ? "zh-CN" : "en";
  }, [locale]);

  useEffect(() => {
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduced || demoTouched) return;
    const words = highlightedWords;
    let index = Math.max(0, words.indexOf(activeWord));
    const timer = window.setInterval(() => {
      index = (index + 1) % words.length;
      setActiveWord(words[index]);
    }, 2200);
    return () => window.clearInterval(timer);
  }, [activeWord, demoTouched, highlightedWords]);

  useEffect(() => {
    if (!highlightedWords.includes(activeWord)) {
      setActiveWord(highlightedWords[0]);
    }
  }, [activeWord, highlightedWords]);

  const changeLocale = (next: Locale) => {
    setLocale(next);
    setDemoTouched(true);
  };

  const handleStageMove = (event: PointerEvent<HTMLDivElement>) => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const x = (event.clientX - rect.left) / rect.width - 0.5;
    const y = (event.clientY - rect.top) / rect.height - 0.5;
    event.currentTarget.style.setProperty("--tilt-x", `${y * -3.4}deg`);
    event.currentTarget.style.setProperty("--tilt-y", `${x * 4.8}deg`);
  };

  const resetStage = () => {
    stageRef.current?.style.setProperty("--tilt-x", "0deg");
    stageRef.current?.style.setProperty("--tilt-y", "0deg");
  };

  const selectDemoWord = (word: string) => {
    setDemoTouched(true);
    setActiveWord(word);
  };

  return (
    <div className="site-shell">
      <a className="skip-link" href="#main">
        {locale === "zh" ? "跳到主要内容" : "Skip to content"}
      </a>

      <header className="topbar">
        <a
          className="brand"
          href={`${PUBLIC_BASE_PATH}/`}
          aria-label={locale === "zh" ? "返回作品集" : "Back to portfolio"}
        >
          <img src={publicAsset("/yujie-logo.png")} alt="" width="46" height="46" />
          <span className="brand-copy">
            <strong>语界精读</strong>
            <small>YUJIE READER</small>
          </span>
        </a>

        <nav className="desktop-nav" aria-label={locale === "zh" ? "主要导航" : "Primary navigation"}>
          {navItems.map((item) => (
            <a href={item.href} key={item.href}>
              {pick(item.label)}
            </a>
          ))}
        </nav>

        <div className="header-actions">
          <div className="locale-switch" role="group" aria-label={locale === "zh" ? "选择语言" : "Choose language"}>
            <button
              type="button"
              className={locale === "zh" ? "active" : ""}
              aria-pressed={locale === "zh"}
              onClick={() => changeLocale("zh")}
            >
              中
            </button>
            <button
              type="button"
              className={locale === "en" ? "active" : ""}
              aria-pressed={locale === "en"}
              onClick={() => changeLocale("en")}
            >
              EN
            </button>
          </div>
          <a className="header-download" href={RELEASE.zipUrl}>
            <span>{locale === "zh" ? "免费下载" : "Download"}</span>
            <span aria-hidden="true">↘</span>
          </a>
        </div>
      </header>

      <main id="main">
        <section className="hero" id="top">
          <div className="hero-orbit orbit-one" aria-hidden="true" />
          <div className="hero-orbit orbit-two" aria-hidden="true" />
          <div className="hero-noise" aria-hidden="true" />

          <div className="hero-copy reveal is-visible">
            <div className="eyebrow">
              <span className="eyebrow-dot" />
              {locale === "zh" ? "Windows 英语精读工具" : "A close-reading studio for Windows"}
            </div>
            <h1>
              {locale === "zh" ? (
                <>
                  把一篇英文文章
                  <br />
                  读成自己的
                  <span>语言地图</span>
                </>
              ) : (
                <>
                  Turn any English article
                  <br />
                  into a personal
                  <span>language map</span>
                </>
              )}
            </h1>
            <p className="hero-lead">
              {locale === "zh"
                ? "按学生年级标出生词，补充释义、注音和句段讲解，再把成果直接变成可打印、可分享的精读讲义。"
                : "Highlight difficult words by student level, add definitions and pronunciation, then turn the result into a polished handout ready to print or share."}
            </p>

            <div className="hero-ctas">
              <a className="button button-primary" href={RELEASE.zipUrl}>
                <span className="button-kicker">Windows 10 / 11 · x64</span>
                <span>
                  {locale === "zh" ? "下载便携版" : "Download portable ZIP"}
                  <b aria-hidden="true">↘</b>
                </span>
              </a>
              <a className="button button-secondary" href="#demo">
                <span className="play-mark" aria-hidden="true">▶</span>
                {locale === "zh" ? "试一试精读演示" : "Try the live demo"}
              </a>
            </div>

            <div className="trust-row" aria-label={locale === "zh" ? "产品特点" : "Product highlights"}>
              <span><i>✓</i>{locale === "zh" ? "无需安装" : "No installation"}</span>
              <span><i>✓</i>{locale === "zh" ? "离线英汉词典" : "Offline dictionary"}</span>
              <span><i>✓</i>{locale === "zh" ? "草稿本地保存" : "Local drafts"}</span>
            </div>
          </div>

          <div
            className="hero-stage reveal is-visible"
            ref={stageRef}
            onPointerMove={handleStageMove}
            onPointerLeave={resetStage}
          >
            <div className="language-orbit" aria-hidden="true">
              <span className="language-chip chip-zh">中文</span>
              <span className="language-chip chip-en">English</span>
              <span className="language-chip chip-es">Español</span>
              <span className="language-chip chip-ja">日本語</span>
              <span className="language-chip chip-fr">Français</span>
            </div>

            <div className="reading-window">
              <div className="window-chrome">
                <div className="window-brand">
                  <img src={publicAsset("/yujie-logo.png")} alt="" width="28" height="28" />
                  <span>{locale === "zh" ? "语界精读 · 实时预览" : "Yujie Reader · Live preview"}</span>
                </div>
                <div className="window-dots" aria-hidden="true"><i /><i /><i /></div>
              </div>

              <div className="window-toolbar" aria-hidden="true">
                <span>初一 / G7</span>
                <span>简体中文</span>
                <span>US IPA</span>
                <b>82%</b>
              </div>

              <div className="reading-paper">
                <div className="paper-meta">
                  <span>LESSON 07</span>
                  <span>4 MIN READ</span>
                </div>
                <h2>The Tea Rose</h2>
                <p className="paper-subtitle">A small garden, quietly opening to the world.</p>
                <p className="sample-paragraph">
                  The tea rose climbed over the{" "}
                  <button
                    type="button"
                    className={activeWord === "weathered" ? "annotated active" : "annotated"}
                    onClick={() => selectDemoWord("weathered")}
                  >
                    weathered
                  </button>{" "}
                  garden wall, its{" "}
                  <button
                    type="button"
                    className={activeWord === "delicate" ? "annotated active" : "annotated"}
                    onClick={() => selectDemoWord("delicate")}
                  >
                    delicate
                  </button>{" "}
                  petals catching the{" "}
                  <button
                    type="button"
                    className={activeWord === "amber" ? "annotated active" : "annotated"}
                    onClick={() => selectDemoWord("amber")}
                  >
                    amber
                  </button>{" "}
                  light.
                </p>
                <p className="sample-paragraph secondary">
                  By dusk, each bud had unfurled, leaving a lingering fragrance in the quiet air.
                </p>

                <div className="definition-card" key={activeWord}>
                  <div>
                    <span className="definition-word">{activeWord}</span>
                    <span className="definition-type">{pick(activeEntry.type)}</span>
                  </div>
                  <span className="definition-ipa">{activeEntry.ipa}</span>
                  <strong>{activeEntry.definitions.zh}</strong>
                  <small>{locale === "zh" ? "点击高亮词查看词条" : "Select a highlighted word to inspect it"}</small>
                </div>

                <div className="paper-footer">
                  <span><i className="status-dot" />{locale === "zh" ? "预览已同步" : "Preview synced"}</span>
                  <span>DOCX</span>
                  <span>PDF</span>
                </div>
              </div>
            </div>

            <div className="floating-note note-top" aria-hidden="true">
              <span>98</span>
              {locale === "zh" ? "种界面语言" : "UI languages"}
            </div>
            <div className="floating-note note-bottom" aria-hidden="true">
              <span>20</span>
              {locale === "zh" ? "个草稿版本" : "draft versions"}
            </div>
          </div>

          <a className="scroll-cue" href="#demo" aria-label={locale === "zh" ? "向下查看" : "Scroll down"}>
            <span>{locale === "zh" ? "向下读" : "READ ON"}</span>
            <i aria-hidden="true" />
          </a>
        </section>

        <section className="language-ribbon" aria-label={locale === "zh" ? "支持的语言示例" : "Supported language examples"}>
          <div className="ribbon-track">
            {[0, 1].map((copyIndex) => (
              <div className="ribbon-set" aria-hidden={copyIndex === 1} key={copyIndex}>
                <span>中文</span><i>✦</i><span>English</span><i>✦</i><span>Español</span><i>✦</i>
                <span>العربية</span><i>✦</i><span>Français</span><i>✦</i><span>日本語</span><i>✦</i>
                <span>Português</span><i>✦</i><span>Deutsch</span><i>✦</i><span>한국어</span><i>✦</i>
              </div>
            ))}
          </div>
        </section>

        <section className="demo-section section-pad" id="demo">
          <div className="section-heading reveal">
            <div>
              <span className="section-index">01 / LIVE DEMO</span>
              <h2>
                {locale === "zh" ? (
                  <>同一篇文章，<span>适合不同的你</span></>
                ) : (
                  <>One article, <span>tuned to each learner</span></>
                )}
              </h2>
            </div>
            <p>
              {locale === "zh"
                ? "切换学生年级，看看标注密度如何变化；再点一个高亮词，观察释义与注音如何跟随。"
                : "Change the student level to see annotation density adapt, then select a word to inspect its definition and pronunciation."}
            </p>
          </div>

          <div className="demo-studio reveal">
            <div className="demo-controls">
              <div className="control-group">
                <span>{locale === "zh" ? "学生年级" : "Student level"}</span>
                <div className="segmented-control">
                  {[4, 7, 10].map((value) => (
                    <button
                      key={value}
                      type="button"
                      className={level === value ? "active" : ""}
                      aria-pressed={level === value}
                      onClick={() => {
                        setDemoTouched(true);
                        setLevel(value);
                      }}
                    >
                      {locale === "zh"
                        ? value === 4 ? "小四" : value === 7 ? "初一" : "高一"
                        : value === 4 ? "Grade 4" : value === 7 ? "Grade 7" : "Grade 10"}
                    </button>
                  ))}
                </div>
              </div>

              <div className="control-group">
                <span>{locale === "zh" ? "释义语言" : "Definition language"}</span>
                <div className="segmented-control language-control">
                  {([
                    ["zh", "中文"],
                    ["ja", "日本語"],
                    ["es", "Español"],
                  ] as Array<[DefinitionLanguage, string]>).map(([value, label]) => (
                    <button
                      key={value}
                      type="button"
                      className={definitionLanguage === value ? "active" : ""}
                      aria-pressed={definitionLanguage === value}
                      onClick={() => {
                        setDemoTouched(true);
                        setDefinitionLanguage(value);
                      }}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>

              <div className="annotation-count">
                <strong>{highlightedWords.length}</strong>
                <span>{locale === "zh" ? "个重点词" : "focus words"}</span>
              </div>
            </div>

            <div className="demo-workspace">
              <article className="demo-article">
                <div className="demo-article-meta">
                  <span>Reading practice</span>
                  <span>{level === 4 ? "P4" : level === 7 ? "M1" : "H1"}</span>
                </div>
                <h3>The Tea Rose</h3>
                <p>
                  {articleTokens.map((token, index) => {
                    const key = cleanToken(token);
                    const highlighted = highlightedWords.includes(key);
                    if (!highlighted) return <span key={`${token}-${index}`}>{token}{" "}</span>;
                    return (
                      <button
                        type="button"
                        key={`${token}-${index}`}
                        className={activeWord === key ? "demo-word active" : "demo-word"}
                        style={{ "--word-delay": `${index * 35}ms` } as CSSProperties}
                        onClick={() => selectDemoWord(key)}
                      >
                        {token}
                        <span className="word-dot" aria-hidden="true" />
                        {" "}
                      </button>
                    );
                  })}
                </p>
                <div className="analysis-note">
                  <span>{locale === "zh" ? "句子提示" : "Sentence note"}</span>
                  <p>
                    {locale === "zh"
                      ? "现在分词 catching… 补充说明花瓣在光线中的状态，让画面从静态变得有动作。"
                      : "The present participle “catching…” adds a moving detail to the petals and turns a still image into a scene."}
                  </p>
                </div>
              </article>

              <aside className="demo-inspector" aria-live="polite">
                <div className="inspector-label">
                  <span>{locale === "zh" ? "本地词典" : "Local dictionary"}</span>
                  <i>{definitionLanguage.toUpperCase()}</i>
                </div>
                <div className="inspector-entry" key={`${activeWord}-${definitionLanguage}`}>
                  <div className="entry-heading">
                    <h4>{activeWord}</h4>
                    <button type="button" aria-label={locale === "zh" ? "播放发音示意" : "Play pronunciation sample"}>◖))</button>
                  </div>
                  <p className="entry-ipa">{activeEntry.ipa}</p>
                  <p className="entry-definition">{activeEntry.definitions[definitionLanguage]}</p>
                  <span className="entry-type">{pick(activeEntry.type)}</span>
                  <div className="entry-example">
                    <small>{locale === "zh" ? "原句定位" : "In context"}</small>
                    <p>“…a <mark>{activeWord}</mark> fragrance in the quiet air.”</p>
                  </div>
                </div>
                <div className="inspector-footer">
                  <span><i />{locale === "zh" ? "本地数据" : "Local data"}</span>
                  <span>{locale === "zh" ? "释义已同步" : "Definition synced"}</span>
                </div>
              </aside>
            </div>
          </div>
        </section>

        <section className="features-section section-pad" id="features">
          <div className="section-heading light reveal">
            <div>
              <span className="section-index">02 / WHY YUJIE</span>
              <h2>
                {locale === "zh" ? (
                  <>不是多查几个词，<span>而是多读懂一层</span></>
                ) : (
                  <>Not more lookups—<span>one deeper layer of reading</span></>
                )}
              </h2>
            </div>
            <p>
              {locale === "zh"
                ? "把词汇、发音、句法、批注和排版收进同一个工作流，让精读从零散动作变成完整作品。"
                : "Bring vocabulary, pronunciation, syntax, notes, and layout into one continuous close-reading workflow."}
            </p>
          </div>

          <div className="feature-grid">
            {features.map((feature, index) => (
              <article
                className={`feature-card feature-${feature.accent} reveal`}
                key={feature.number}
                style={{ "--card-delay": `${index * 70}ms` } as CSSProperties}
              >
                <div className="feature-top">
                  <span>{feature.number}</span>
                  <strong>{feature.metric}</strong>
                </div>
                <h3>{pick(feature.title)}</h3>
                <p>{pick(feature.body)}</p>
                <div className="feature-line" aria-hidden="true" />
              </article>
            ))}
          </div>
        </section>

        <section className="workflow-section section-pad" id="workflow">
          <div className="workflow-intro reveal">
            <span className="section-index">03 / WORKFLOW</span>
            <h2>
              {locale === "zh" ? (
                <>三步，把原文<br />变成<span>自己的讲义</span></>
              ) : (
                <>Three steps from<br />article to <span>your handout</span></>
              )}
            </h2>
            <p>
              {locale === "zh"
                ? "不需要学习一套复杂系统。把你已经在读的内容带进来，剩下的交给清晰的流程。"
                : "No complicated system to learn. Bring in what you are already reading and follow a focused path."}
            </p>
          </div>

          <div className="workflow-list">
            {workflow.map((item, index) => (
              <article className="workflow-item reveal" key={item.step}>
                <div className="workflow-number">
                  <span>{item.step}</span>
                  <i aria-hidden="true">{index === 0 ? "↳" : index === 1 ? "✦" : "✓"}</i>
                </div>
                <div className="workflow-copy">
                  <small>{pick(item.note)}</small>
                  <h3>{pick(item.title)}</h3>
                  <p>{pick(item.body)}</p>
                </div>
                <div className={`workflow-visual visual-${index + 1}`} aria-hidden="true">
                  {index === 0 && (
                    <>
                      <span className="mini-line wide" /><span className="mini-line" /><span className="mini-line medium" />
                      <span className="mini-cursor">|</span>
                    </>
                  )}
                  {index === 1 && (
                    <>
                      <span className="level-tag">P4</span><span className="level-tag active">M1</span><span className="level-tag">H1</span>
                      <span className="highlight-stroke">context</span>
                    </>
                  )}
                  {index === 2 && (
                    <>
                      <span className="document-sheet docx">W</span>
                      <span className="document-sheet pdf">PDF</span>
                      <span className="export-arrow">↗</span>
                    </>
                  )}
                </div>
              </article>
            ))}
          </div>
        </section>

        <section className="manifesto">
          <div className="manifesto-ring ring-a" aria-hidden="true" />
          <div className="manifesto-ring ring-b" aria-hidden="true" />
          <div className="manifesto-content reveal">
            <img src={publicAsset("/yujie-logo.png")} width="112" height="112" alt="" />
            <span>{locale === "zh" ? "在语境中，读懂世界" : "Read the world, one context at a time"}</span>
            <h2>
              {locale === "zh"
                ? "理解，不该被生词挡住。"
                : "Understanding should not stop at an unfamiliar word."}
            </h2>
            <p>
              {locale === "zh"
                ? "语界精读让每篇真实文章都能贴近学习者当下的水平，同时保留继续探索的空间。"
                : "Yujie Reader brings authentic articles closer to each learner’s current level without flattening the world inside them."}
            </p>
          </div>
        </section>

        <section className="download-section section-pad" id="download">
          <div className="download-shell reveal">
            <div className="download-copy">
              <span className="section-index">04 / DOWNLOAD</span>
              <h2>
                {locale === "zh" ? (
                  <>一次下载，<br /><span>开箱即读</span></>
                ) : (
                  <>Download once.<br /><span>Start reading.</span></>
                )}
              </h2>
              <p>
                {locale === "zh"
                  ? `语界精读 v${RELEASE.version} 是 Windows 10/11 便携软件，无需安装或注册。下载 ZIP 后解压并运行即可。`
                  : `Yujie Reader v${RELEASE.version} is a portable app for Windows 10/11. No installation or account is required.`}
              </p>
              <div className="release-meta">
                <span>v{RELEASE.version}</span>
                <span>{RELEASE.date}</span>
                <span>Windows x64</span>
              </div>
            </div>

            <div className="download-panel">
              <a className="download-option recommended" href={RELEASE.zipUrl}>
                <div className="download-icon">ZIP</div>
                <div>
                  <span className="recommend-badge">{locale === "zh" ? "推荐" : "Recommended"}</span>
                  <strong>{locale === "zh" ? "Windows 便携包" : "Windows portable package"}</strong>
                  <small>{locale === "zh" ? "含使用说明与第三方许可" : "Includes guide and third-party notices"}</small>
                </div>
                <div className="download-size">
                  <span>{RELEASE.zipSize}</span>
                  <b aria-hidden="true">↘</b>
                </div>
              </a>

              <a className="download-option" href={RELEASE.exeUrl}>
                <div className="download-icon exe">EXE</div>
                <div>
                  <strong>{locale === "zh" ? "Windows 单文件版" : "Windows standalone EXE"}</strong>
                  <small>{locale === "zh" ? "无需解压，双击运行" : "Run directly without extracting"}</small>
                </div>
                <div className="download-size">
                  <span>{RELEASE.exeSize}</span>
                  <b aria-hidden="true">↘</b>
                </div>
              </a>

              <div className="download-links">
                <a href={RELEASE.releaseUrl}>{locale === "zh" ? "查看更新日志" : "Release notes"} ↗</a>
                <a href={RELEASE.checksumUrl}>SHA-256 ↗</a>
                <a href={RELEASE.latestUrl}>{locale === "zh" ? "全部版本" : "All releases"} ↗</a>
              </div>

              <details className="checksum-details">
                <summary>{locale === "zh" ? "显示文件校验值" : "Show file checksums"}</summary>
                <div>
                  <span>Portable ZIP</span>
                  <code>{RELEASE.zipSha}</code>
                  <span>Standalone EXE</span>
                  <code>{RELEASE.exeSha}</code>
                </div>
              </details>
            </div>
          </div>

          <div className="requirements-grid reveal">
            <div>
              <span>01</span>
              <strong>{locale === "zh" ? "系统" : "System"}</strong>
              <p>Windows 10 / 11 · 64-bit</p>
            </div>
            <div>
              <span>02</span>
              <strong>WebView2</strong>
              <p>{locale === "zh" ? "Windows 10/11 通常已内置" : "Usually included with Windows 10/11"}</p>
            </div>
            <div>
              <span>03</span>
              <strong>PDF</strong>
              <p>{locale === "zh" ? "需要 Edge 或 Chrome；DOCX 可离线" : "Requires Edge or Chrome; DOCX works offline"}</p>
            </div>
            <div className="signature-note">
              <span>!</span>
              <strong>{locale === "zh" ? "下载提醒" : "Download notice"}</strong>
              <p>
                {locale === "zh"
                  ? "当前版本未做数字签名，Windows 可能显示安全提醒。请仅从本站或官方 GitHub Release 下载。"
                  : "This release is not yet digitally signed, so Windows may show a warning. Download only from this site or the official GitHub Release."}
              </p>
            </div>
          </div>
        </section>

        <section className="faq-section section-pad" id="faq">
          <div className="faq-heading reveal">
            <span className="section-index">05 / FAQ</span>
            <h2>{locale === "zh" ? "下载之前，先把疑问读完" : "A few answers before you download"}</h2>
            <p>
              {locale === "zh"
                ? "我们尽量把联网范围、系统条件和当前版本状态说清楚。"
                : "Clear details about connectivity, system requirements, and the current release."}
            </p>
          </div>
          <div className="faq-list reveal">
            {faqs.map((faq, index) => (
              <details key={faq.question.zh} open={index === 0}>
                <summary>
                  <span>{String(index + 1).padStart(2, "0")}</span>
                  <strong>{pick(faq.question)}</strong>
                  <i aria-hidden="true">＋</i>
                </summary>
                <p>{pick(faq.answer)}</p>
              </details>
            ))}
          </div>
        </section>

        <section className="privacy-strip">
          <div>
            <span>{locale === "zh" ? "本地优先，但不是完全离线" : "Local-first, not fully offline"}</span>
            <p>
              {locale === "zh"
                ? "文章解析、分级标注、简体中文释义和 IPA 在设备上完成。选择其他目标语言时，软件可能向第三方翻译服务发送最多 48 个待标注英文词，不发送整篇文章。"
                : "Article analysis, level-based annotation, Chinese definitions, and IPA happen on your device. For other target languages, the app may send up to 48 vocabulary candidates—not the full article—to third-party translation services."}
            </p>
          </div>
          <a href="https://github.com/zkmaojack/cijing-reader/blob/main/THIRD_PARTY_NOTICES.md">
            {locale === "zh" ? "第三方许可" : "Third-party notices"} ↗
          </a>
        </section>
      </main>

      <footer>
        <div className="footer-brand">
          <img src={publicAsset("/yujie-logo.png")} alt="" width="58" height="58" />
          <div>
            <strong>语界精读</strong>
            <span>Yujie Reader</span>
          </div>
        </div>
        <p>{locale === "zh" ? "在语境中，读懂世界。" : "Read the world, one context at a time."}</p>
        <div className="footer-links">
          <a href={RELEASE.latestUrl}>GitHub</a>
          <a href={RELEASE.releaseUrl}>{locale === "zh" ? "更新日志" : "Changelog"}</a>
          <a href="#faq">FAQ</a>
          <a href="#top">{locale === "zh" ? "回到顶部 ↑" : "Back to top ↑"}</a>
        </div>
        <small>© 2026 Yujie Reader · v{RELEASE.version}</small>
      </footer>

      <a className="mobile-download-bar" href={RELEASE.zipUrl}>
        <div>
          <span>v{RELEASE.version} · {RELEASE.zipSize}</span>
          <strong>{locale === "zh" ? "下载 Windows 便携版" : "Download for Windows"}</strong>
        </div>
        <b aria-hidden="true">↘</b>
      </a>
    </div>
  );
}
