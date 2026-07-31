import type { Metadata } from "next";
import styles from "./airflow.module.css";

export const dynamic = "force-static";

const basePath = (process.env.NEXT_PUBLIC_BASE_PATH ?? "").replace(/\/$/, "");
const portfolioHome = `${basePath}/`;
const publicAsset = (pathname: string) => `${basePath}${pathname}`;

const portableUrl =
  "https://github.com/zkmaojack/FlowLab-2D/releases/download/v0.1.1/FlowLab-2D.exe";
const installerUrl =
  "https://github.com/zkmaojack/FlowLab-2D/releases/download/v0.1.1/FlowLab-2D-Setup-x64.exe";
const releaseUrl =
  "https://github.com/zkmaojack/FlowLab-2D/releases/tag/v0.1.1";
const sourceUrl = "https://github.com/zkmaojack/FlowLab-2D";

export const metadata: Metadata = {
  title: "FlowLab 2D｜实时二维流体实验台",
  description:
    "FlowLab 2D v0.1.1 是一款本地运行的 D2Q9 格子玻尔兹曼二维流体教育模拟器，支持实时流线、涡量、压力与气动力观测。",
  keywords: [
    "FlowLab 2D",
    "二维流体模拟",
    "格子玻尔兹曼",
    "D2Q9",
    "流体力学教育",
    "Windows",
  ],
  alternates: {
    canonical: `${basePath}/airflow/`,
  },
  openGraph: {
    type: "website",
    locale: "zh_CN",
    title: "FlowLab 2D｜看见流动，理解空气动力学",
    description:
      "实时绘制边界、切换流场视图，并在桌面端探索阻力、升力与雷诺数。",
    url: `${basePath}/airflow/`,
    images: [
      {
        url: publicAsset("/flowlab-preview.png"),
        alt: "FlowLab 2D 实时二维流体模拟界面",
      },
    ],
  },
  twitter: {
    card: "summary_large_image",
    title: "FlowLab 2D v0.1.1",
    description: "A real-time 2D flow laboratory, built for learning.",
    images: [publicAsset("/flowlab-preview.png")],
  },
};

const featureGroups = [
  {
    number: "01",
    title: "从空白画布开始塑造流场",
    english: "DRAW THE BOUNDARY",
    description:
      "直接绘制、擦除或撤销障碍物，也可以从圆柱、翼型、汽车等示例几何开始实验。鼠标、触控与键盘操作都经过适配。",
    tags: ["绘制 / 擦除 / 撤销", "圆柱", "翼型", "汽车"],
  },
  {
    number: "02",
    title: "在不同视角里看懂同一股流动",
    english: "READ THE FLOW",
    description:
      "在速度、涡量与压力视图之间切换；Windy 风格的动态粒子轨迹把分离、尾流和旋涡变成可观察的运动。",
    tags: ["速度场", "涡量", "压力", "动态轨迹"],
  },
  {
    number: "03",
    title: "让直觉落到可读的量上",
    english: "MEASURE THE FORCE",
    description:
      "用风速探针读取局部速度，同时观察阻力、升力、Cd、Cl 与 Re，并通过单位换算连接模拟量和现实尺度。",
    tags: ["风速探针", "Drag / Lift", "Cd / Cl", "Re"],
  },
  {
    number: "04",
    title: "不止空气，也不止一种参数",
    english: "CHANGE THE MEDIUM",
    description:
      "快速选择空气或水，也可以定义自有流体参数。每一次调整都会即时反馈，让参数变化与流场响应建立清晰联系。",
    tags: ["空气", "水", "自定义流体", "实时反馈"],
  },
] as const;

const workflow = [
  {
    step: "01",
    title: "选择介质",
    english: "Choose a medium",
    copy: "从空气、水或自定义参数开始，设置你想观察的流动环境。",
  },
  {
    step: "02",
    title: "画出边界",
    english: "Shape the scene",
    copy: "手绘障碍物，或载入圆柱、翼型和汽车示例，随时擦除与撤销。",
  },
  {
    step: "03",
    title: "读取流场",
    english: "Read the flow",
    copy: "切换视图、放置探针，在实时轨迹和气动力数据中验证你的判断。",
  },
] as const;

export default function AirflowPage() {
  return (
    <div className={styles.flowPage}>
      <a className={styles.flowSkipLink} href="#flow-main">
        跳到主要内容
      </a>

      <header className={styles.flowHeader}>
        <a
          className={styles.flowBrand}
          href={portfolioHome}
          aria-label="返回 Jack Mao 个人作品首页"
        >
          <span className={styles.flowBrandMark}>
            <img
              src={publicAsset("/flowlab-logo.svg")}
              width="40"
              height="40"
              alt=""
            />
          </span>
          <span className={styles.flowBrandCopy}>
            <strong>FlowLab 2D</strong>
            <small>REAL-TIME FLOW LAB</small>
          </span>
        </a>

        <nav className={styles.flowNav} aria-label="FlowLab 页面导航">
          <a href="#capabilities">能力</a>
          <a href="#workflow">使用方式</a>
          <a href="#download">下载</a>
        </nav>

        <a className={styles.flowHomeLink} href={portfolioHome}>
          <span aria-hidden="true">←</span>
          <span>作品首页</span>
        </a>
      </header>

      <main id="flow-main">
        <section className={styles.flowHero} aria-labelledby="flow-hero-title">
          <div className={styles.flowGrid} aria-hidden="true" />
          <div className={styles.flowHeroCopy}>
            <div className={styles.flowKicker}>
              <span className={styles.flowLiveDot} aria-hidden="true" />
              <span>FLOWLAB 2D · VERSION 0.1.1</span>
            </div>

            <h1 id="flow-hero-title">
              让风穿过你画下的
              <span>每一道边界。</span>
            </h1>
            <p className={styles.flowHeroLead}>
              一座运行在桌面上的实时二维流体实验台。画出形状、改变介质，
              然后在速度、涡量、压力与气动力数据中看见答案。
            </p>
            <p className={styles.flowHeroEnglish}>
              Shape the boundary. Watch the flow. Build intuition.
            </p>

            <div className={styles.flowHeroActions}>
              <a
                className={styles.flowPrimaryButton}
                href={installerUrl}
                aria-label="下载 FlowLab 2D Windows 安装版，3.50 MB"
              >
                <span>
                  <small>WINDOWS 10 / 11 · X64</small>
                  <strong>下载安装版</strong>
                </span>
                <b aria-hidden="true">↓</b>
              </a>
              <a
                className={styles.flowSecondaryButton}
                href={sourceUrl}
                target="_blank"
                rel="noreferrer"
              >
                查看源代码
                <span aria-hidden="true">↗</span>
              </a>
            </div>

            <ul className={styles.flowHeroFacts} aria-label="产品特性摘要">
              <li>本地优先</li>
              <li>离线可用</li>
              <li>D2Q9 LBM</li>
              <li>鼠标 / 触控 / 键盘</li>
            </ul>
          </div>

          <div className={styles.flowHeroVisual}>
            <figure className={styles.flowScreen}>
              <div className={styles.flowScreenBar} aria-hidden="true">
                <div className={styles.flowWindowDots}>
                  <i />
                  <i />
                  <i />
                </div>
                <span>LIVE SIMULATION / VELOCITY FIELD</span>
                <b>60 FPS</b>
              </div>
              <div className={styles.flowScreenImage}>
                <img
                  src={publicAsset("/flowlab-preview.png")}
                  width="1600"
                  height="1000"
                  alt="FlowLab 2D 界面，展示物体周围的实时二维流场与动态粒子轨迹"
                  fetchPriority="high"
                />
                <div className={styles.flowScanLine} aria-hidden="true" />
              </div>
              <figcaption>
                <span>Simulation running locally</span>
                <span>Privacy by default</span>
              </figcaption>
            </figure>

            <aside
              className={`${styles.flowMetricCard} ${styles.flowMetricTop}`}
              aria-label="实时测量示例"
            >
              <span>WIND PROBE</span>
              <strong>12.4 m/s</strong>
              <small>实时局部风速</small>
            </aside>
            <aside
              className={`${styles.flowMetricCard} ${styles.flowMetricBottom}`}
              aria-label="气动力系数示例"
            >
              <span>AERO COEFFICIENTS</span>
              <div>
                <b>Cd&nbsp; 0.31</b>
                <b>Cl&nbsp; 0.08</b>
              </div>
              <small>Drag · Lift · Reynolds</small>
            </aside>
          </div>
        </section>

        <div className={styles.flowSignalStrip} aria-label="FlowLab 技术摘要">
          <span>D2Q9 LATTICE BOLTZMANN</span>
          <i aria-hidden="true" />
          <span>REAL-TIME VISUALIZATION</span>
          <i aria-hidden="true" />
          <span>LOCAL-FIRST DESKTOP</span>
          <i aria-hidden="true" />
          <span>BUILT FOR CURIOSITY</span>
        </div>

        <section
          className={`${styles.flowSection} ${styles.flowIntro}`}
          aria-labelledby="flow-intro-title"
        >
          <div className={styles.flowSectionEyebrow}>
            <span>01</span>
            <p>WHAT IT IS</p>
          </div>
          <div className={styles.flowIntroCopy}>
            <h2 id="flow-intro-title">
              把抽象的流体力学，变成一场可以动手的实验。
            </h2>
            <p>
              FlowLab 2D 使用 D2Q9 格子玻尔兹曼方法进行实时二维计算。
              它没有把探索藏在复杂菜单里：你画出的每一条边界、调整的每一个参数，
              都会直接成为屏幕上的流动。
            </p>
          </div>
          <dl className={styles.flowStatRail}>
            <div>
              <dt>3</dt>
              <dd>流场视图</dd>
              <small>Speed · Vorticity · Pressure</small>
            </div>
            <div>
              <dt>3+</dt>
              <dd>示例几何</dd>
              <small>Cylinder · Airfoil · Car</small>
            </div>
            <div>
              <dt>100%</dt>
              <dd>本地运行</dd>
              <small>Offline desktop experience</small>
            </div>
          </dl>
        </section>

        <section
          id="capabilities"
          className={`${styles.flowSection} ${styles.flowCapabilities}`}
          aria-labelledby="flow-capabilities-title"
        >
          <div className={styles.flowSectionHeading}>
            <div>
              <span className={styles.flowMiniLabel}>CAPABILITIES / 02</span>
              <h2 id="flow-capabilities-title">一块画布，四种探索方式。</h2>
            </div>
            <p>
              从几何、视图、数据到介质参数，所有工具都围绕一个目标：
              让“为什么会这样流”更容易被看见和验证。
            </p>
          </div>

          <div className={styles.flowFeatureGrid}>
            {featureGroups.map((feature) => (
              <article className={styles.flowFeatureCard} key={feature.number}>
                <div className={styles.flowFeatureTopline}>
                  <span>{feature.number}</span>
                  <small>{feature.english}</small>
                </div>
                <h3>{feature.title}</h3>
                <p>{feature.description}</p>
                <ul aria-label={`${feature.title}功能`}>
                  {feature.tags.map((tag) => (
                    <li key={tag}>{tag}</li>
                  ))}
                </ul>
              </article>
            ))}
          </div>
        </section>

        <section
          id="workflow"
          className={`${styles.flowSection} ${styles.flowWorkflow}`}
          aria-labelledby="flow-workflow-title"
        >
          <div className={styles.flowWorkflowHeading}>
            <span className={styles.flowMiniLabel}>HOW IT FLOWS / 03</span>
            <h2 id="flow-workflow-title">三步，从想象走到流场。</h2>
            <p>无需联网，也无需准备复杂的工程模型。</p>
          </div>

          <ol className={styles.flowWorkflowList}>
            {workflow.map((item) => (
              <li key={item.step}>
                <span className={styles.flowWorkflowNumber}>{item.step}</span>
                <div>
                  <small>{item.english}</small>
                  <h3>{item.title}</h3>
                  <p>{item.copy}</p>
                </div>
              </li>
            ))}
          </ol>

          <div className={styles.flowMethodPanel}>
            <div className={styles.flowMethodCopy}>
              <span>UNDER THE HOOD</span>
              <h3>D2Q9 格子玻尔兹曼方法</h3>
              <p>
                九个离散速度方向在二维网格中传播与碰撞，
                以适合实时交互的方式近似流体演化。
                这里的目标不是替代专业 CFD，而是缩短概念与直觉之间的距离。
              </p>
            </div>
            <div className={styles.flowLattice} aria-hidden="true">
              <span className={styles.flowLatticeCenter} />
              <i />
              <i />
              <i />
              <i />
              <i />
              <i />
              <i />
              <i />
            </div>
          </div>

          <aside className={styles.flowNotice} aria-labelledby="flow-notice-title">
            <span className={styles.flowNoticeMark} aria-hidden="true">
              !
            </span>
            <div>
              <strong id="flow-notice-title">教育用途说明</strong>
              <p>
                FlowLab 2D 用于教学和概念比较，以及直观探索；
                不替代工程级三维 CFD、风洞试验、认证分析或安全评估。
              </p>
            </div>
            <small>EDUCATIONAL USE · NOT FOR ENGINEERING DECISIONS</small>
          </aside>
        </section>

        <section
          id="download"
          className={`${styles.flowSection} ${styles.flowDownload}`}
          aria-labelledby="flow-download-title"
        >
          <div className={styles.flowDownloadIntro}>
            <span className={styles.flowMiniLabel}>DOWNLOAD / 04</span>
            <h2 id="flow-download-title">把实验台带到桌面。</h2>
            <p>
              FlowLab 2D 是本地优先的离线桌面应用。下载后，
              模拟与数据都在你的 Windows 设备上运行。
            </p>

            <div className={styles.flowRequirements}>
              <div>
                <span>系统</span>
                <strong>Windows 10 / 11</strong>
              </div>
              <div>
                <span>架构</span>
                <strong>x64</strong>
              </div>
              <div>
                <span>运行环境</span>
                <strong>Microsoft WebView2</strong>
              </div>
            </div>
          </div>

          <div className={styles.flowDownloadPanel}>
            <a
              className={`${styles.flowDownloadCard} ${styles.flowRecommended}`}
              href={installerUrl}
              aria-label="下载 FlowLab 2D v0.1.1 Windows 安装版，3.50 MB"
            >
              <div className={styles.flowDownloadIcon} aria-hidden="true">
                SETUP
              </div>
              <div className={styles.flowDownloadCopy}>
                <span>推荐 · RECOMMENDED</span>
                <strong>Windows 安装版</strong>
                <small>FlowLab-2D-Setup-x64.exe</small>
              </div>
              <div className={styles.flowDownloadMeta}>
                <span>3.50 MB</span>
                <b aria-hidden="true">↓</b>
              </div>
            </a>

            <a
              className={styles.flowDownloadCard}
              href={portableUrl}
              aria-label="下载 FlowLab 2D v0.1.1 Windows 便携版，9.60 MB"
            >
              <div
                className={`${styles.flowDownloadIcon} ${styles.flowPortableIcon}`}
                aria-hidden="true"
              >
                EXE
              </div>
              <div className={styles.flowDownloadCopy}>
                <span>PORTABLE</span>
                <strong>Windows 便携版</strong>
                <small>FlowLab-2D.exe · 无需安装</small>
              </div>
              <div className={styles.flowDownloadMeta}>
                <span>9.60 MB</span>
                <b aria-hidden="true">↓</b>
              </div>
            </a>

            <div className={styles.flowReleaseLinks}>
              <a href={releaseUrl} target="_blank" rel="noreferrer">
                查看 v0.1.1 发布说明 <span aria-hidden="true">↗</span>
              </a>
              <a href={sourceUrl} target="_blank" rel="noreferrer">
                GitHub 源代码 <span aria-hidden="true">↗</span>
              </a>
            </div>

            <details className={styles.flowSecurity}>
              <summary>Windows 安全提示与 SHA-256 校验</summary>
              <div>
                <p>
                  v0.1.1 尚未进行 Authenticode 数字签名，Windows SmartScreen
                  可能显示“未知发布者”。请仅从本页或项目的 GitHub Release 下载。
                </p>
                <span>安装版</span>
                <code>34306BF5B1D5B7305752D698D1092C53AB5C4A855BAFB1E8AB956ABA32E37DDE</code>
                <span>便携版</span>
                <code>204B6A0EFF0FF6054EFF225502466AF599C81824A6F677354ADF5A065FD741E7</code>
              </div>
            </details>
          </div>
        </section>
      </main>

      <div className={styles.flowFooter} role="contentinfo">
        <a className={styles.flowFooterBrand} href={portfolioHome}>
          <img
            src={publicAsset("/flowlab-logo.svg")}
            width="48"
            height="48"
            alt=""
          />
          <span>
            <strong>FlowLab 2D</strong>
            <small>A PROJECT BY JACK MAO</small>
          </span>
        </a>
        <p>
          看见流动，建立直觉。
          <small>See the flow. Build intuition.</small>
        </p>
        <div className={styles.flowFooterLinks}>
          <a href={portfolioHome}>全部作品</a>
          <a href={sourceUrl} target="_blank" rel="noreferrer">
            GitHub
          </a>
          <a href="#flow-main">回到顶部 ↑</a>
        </div>
      </div>
    </div>
  );
}
