import styles from "./portfolio.module.css";

export const dynamic = "force-static";

const BASE_PATH = process.env.NEXT_PUBLIC_BASE_PATH ?? "";
const publicAsset = (pathname: string) => `${BASE_PATH}${pathname}`;
const route = (pathname: string) => `${BASE_PATH}${pathname}`;

const links = {
  cijingDownload:
    "https://github.com/zkmaojack/cijing-reader/releases/download/v1.4.1/yujie-reader-v1.4.1-portable.zip",
  flowlabDownload:
    "https://github.com/zkmaojack/FlowLab-2D/releases/download/v0.1.1/FlowLab-2D-Setup-x64.exe",
  github: "https://github.com/zkmaojack",
};

export default function Page() {
  return (
    <div className={styles.page}>
      <a className={styles.skipLink} href="#projects">
        跳到作品
      </a>

      <header className={styles.header}>
        <a className={styles.signature} href={route("/")} aria-label="Jack Mao 作品集首页">
          <span className={styles.signatureMark}>JM</span>
          <span>
            <strong>Jack Mao</strong>
            <small>Independent builder</small>
          </span>
        </a>
        <nav className={styles.nav} aria-label="作品集导航">
          <a href="#projects">作品</a>
          <a href="#about">关于</a>
          <a href={links.github} target="_blank" rel="noreferrer">
            GitHub <span aria-hidden="true">↗</span>
          </a>
        </nav>
      </header>

      <main>
        <section className={styles.hero} aria-labelledby="portfolio-title">
          <div className={styles.heroCopy}>
            <p className={styles.eyebrow}>
              <span>Selected works</span>
              <i aria-hidden="true" />
              2026
            </p>
            <h1 id="portfolio-title">
              把复杂问题，
              <br />
              做成<span>真正好用</span>的工具。
            </h1>
            <p className={styles.heroLead}>
              我关注学习体验、科学计算与桌面软件。这里收录两件正在持续打磨的独立作品：
              一件帮助人读懂语言，一件帮助人看见流动。
            </p>
          </div>

          <aside className={styles.heroNote} aria-label="作品集摘要">
            <span className={styles.noteIndex}>02</span>
            <p>两件作品，共同从真实使用场景出发。</p>
            <div className={styles.noteMeta}>
              <span>Windows</span>
              <span>Local-first</span>
              <span>Built with care</span>
            </div>
          </aside>
        </section>

        <section className={styles.projects} id="projects" aria-labelledby="projects-title">
          <div className={styles.sectionHead}>
            <span>01 / Selected projects</span>
            <h2 id="projects-title">选择一个入口</h2>
            <p>每个项目都有独立的介绍、功能说明与下载方式。</p>
          </div>

          <div className={styles.projectGrid}>
            <article className={`${styles.projectCard} ${styles.readerCard}`}>
              <div className={styles.cardTopline}>
                <span>01 · Learning tool</span>
                <span>v1.4.1</span>
              </div>

              <div className={styles.readerVisual} aria-hidden="true">
                <div className={styles.readerChrome}>
                  <span />
                  <span />
                  <span />
                  <b>CIJING READER</b>
                </div>
                <div className={styles.readerPaper}>
                  <small>THE TEA ROSE · CLOSE READING</small>
                  <strong>Words become a map.</strong>
                  <p>
                    A weathered gate opened into a <mark>delicate</mark> garden, where every sentence
                    carried a little more meaning.
                  </p>
                  <div className={styles.wordNote}>
                    <b>delicate</b>
                    <span>/ˈdel.ɪ.kət/ · 精致的；娇嫩的</span>
                  </div>
                </div>
              </div>

              <div className={styles.cardCopy}>
                <div>
                  <img src={publicAsset("/yujie-logo.png")} alt="" width="54" height="54" />
                  <span>语界精读 · Yujie Reader</span>
                </div>
                <h3>把英文文章变成适合学习者的精读讲义。</h3>
                <p>
                  按年级标出生词，补充多语言释义、IPA 与句段讲解，并导出 DOCX / PDF。
                  一套为教师与学习者准备的本地优先工作流。
                </p>
                <ul aria-label="语界精读特点">
                  <li>分级词汇</li>
                  <li>98 种界面语言</li>
                  <li>无需配置 API</li>
                </ul>
              </div>

              <div className={styles.cardActions}>
                <a className={styles.primaryAction} href={route("/cijing/")}>
                  进入语界精读 <span aria-hidden="true">→</span>
                </a>
                <a href={links.cijingDownload}>下载 Windows 便携版 <span aria-hidden="true">↘</span></a>
              </div>
            </article>

            <article className={`${styles.projectCard} ${styles.flowCard}`}>
              <div className={styles.cardTopline}>
                <span>02 · Simulation tool</span>
                <span>v0.1.1</span>
              </div>

              <div className={styles.flowVisual}>
                <img
                  src={publicAsset("/flowlab-preview.png")}
                  alt="FlowLab 2D 圆柱绕流与涡街可视化"
                />
                <div className={styles.flowReadout} aria-hidden="true">
                  <span>Re</span>
                  <b>18,240</b>
                  <i>LIVE · 60 Hz</i>
                </div>
              </div>

              <div className={styles.cardCopy}>
                <div>
                  <img src={publicAsset("/flowlab-logo.svg")} alt="" width="54" height="54" />
                  <span>FlowLab 2D · CFD / Live</span>
                </div>
                <h3>画出一个物体，然后实时看见空气与水如何绕过它。</h3>
                <p>
                  自由绘制几何，切换速度、涡量与压力场，用探针读取风速，并观察升力、阻力与流态变化。
                  面向教学与概念比较的二维流体工作台。
                </p>
                <ul aria-label="FlowLab 2D 特点">
                  <li>实时流场</li>
                  <li>自由绘制障碍物</li>
                  <li>离线桌面版</li>
                </ul>
              </div>

              <div className={styles.cardActions}>
                <a className={styles.primaryAction} href={route("/airflow/")}>
                  进入 FlowLab 2D <span aria-hidden="true">→</span>
                </a>
                <a href={links.flowlabDownload}>下载安装版 <span aria-hidden="true">↘</span></a>
              </div>
            </article>
          </div>
        </section>

        <section className={styles.about} id="about" aria-labelledby="about-title">
          <p>02 / About the work</p>
          <div>
            <h2 id="about-title">软件不仅要能运行，也应该让人愿意继续使用。</h2>
            <p>
              从界面、交互到发布包，我把每件作品都当作完整产品来做。
              这里不会只陈列概念图；你可以进入项目，了解它，也可以直接下载使用。
            </p>
          </div>
        </section>
      </main>

      <div className={styles.footer} role="contentinfo">
        <div>
          <span className={styles.signatureMark}>JM</span>
          <p>
            <strong>Jack Mao</strong>
            <small>Selected independent works</small>
          </p>
        </div>
        <p>Language × Simulation × Thoughtful software</p>
        <a href={links.github} target="_blank" rel="noreferrer">
          github.com/zkmaojack <span aria-hidden="true">↗</span>
        </a>
      </div>
    </div>
  );
}
