import React, { useEffect, useRef, useState } from 'react';
import { BookOpen, Download, ChevronRight, Check } from 'lucide-react';

/* ─── ANIMATED VERSE ─── */
const VERSE_SEGMENTS = [
  { text: 'And we know that', highlight: false },
  { text: 'all things', highlight: false },
  { text: 'work together', highlight: true },
  { text: 'for good', highlight: true },
  { text: 'to them that love God,', highlight: false },
  { text: 'to them who are the called', highlight: false },
  { text: 'according to his purpose.', highlight: false },
];

function AnimatedVerse() {
  const words: { text: string; highlight: boolean; delay: number }[] = [];
  let i = 0;
  VERSE_SEGMENTS.forEach(({ text, highlight }) => {
    text.split(' ').forEach((w) => {
      if (w) {
        words.push({ text: w, highlight, delay: 0.3 + i * 0.07 });
        i++;
      }
    });
  });

  return (
    <div className="verse-text">
      {words.map(({ text, highlight, delay }, idx) => (
        <span
          key={idx}
          className={`verse-word${highlight ? ' verse-highlight' : ''}`}
          style={{ animationDelay: `${delay}s` }}
        >
          {text}{' '}
        </span>
      ))}
    </div>
  );
}

/* ─── SCROLL-IN HOOK ─── */
function useInView(threshold = 0.3) {
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const obs = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting) setVisible(true);
      },
      { threshold },
    );
    obs.observe(el);
    return () => obs.disconnect();
  }, [threshold]);
  return { ref, visible };
}

/* ═══════════════════════════════════════════════════════ */

export default function LandingPage() {
  return (
    <div>
      <Nav />
      <Hero />
      <ProofTicker />
      <Features />
      <EditorialShowcase />
      <HowItWorks />
      <DownloadSection />
      <Footer />
    </div>
  );
}

/* ─── NAV ─── */
function Nav() {
  return (
    <nav className="nav">
      <a href="/" className="nav-logo">
        <div className="logomark">
          <BookOpen size={16} color="#fff" />
        </div>
        Companion Bible
      </a>
      <ul className="nav-links">
        <li>
          <a href="#features">Features</a>
        </li>
        <li>
          <a href="#how-it-works">How It Works</a>
        </li>
        <li>
          <a href="#download">Download</a>
        </li>
      </ul>
      <a href="#download" className="nav-dl">
        <Download size={14} />
        Free Download
      </a>
    </nav>
  );
}

/* ─── HERO ─────────────────────────────────────────────
   The top portion IS the congregation screen.
   The user sees exactly what the congregation sees.
   Headline + CTA emerge below, out of the darkness.
────────────────────────────────────────────────────── */
function Hero() {
  return (
    <>
      <section className="hero">
        {/* projector beam + grid texture */}
        <div className="hero-beam" aria-hidden="true" />
        <div className="hero-grid" aria-hidden="true" />

        <div className="hero-inner">
          {/* live status */}
          <div className="live-badge a1">
            <div className="live-dot" />
            <Waveform />
            Listening live
          </div>

          {/* congregation screen verse */}
          <div className="congregation-display a2">
            <AnimatedVerse />
            <div className="verse-ref">Romans 8 : 28 · KJV</div>
          </div>
        </div>

        {/* gradient fade — congregation screen bleeds into headline */}
        <div className="hero-fade" aria-hidden="true" />
      </section>

      {/* headline + CTA sit below the hero, outside it so layout is clean */}
      <div className="hero-bottom">
        <h1 className="hero-h1 a3">
          The verse appears
          <br />
          the moment <em>it's spoken.</em>
        </h1>
        <p className="hero-sub a4">
          Companion Bible streams your pastor's sermon live, detects every Bible verse and GHS hymn
          number, and pushes it to the congregation screen in real time. No keyboard. No delay.
        </p>
        <div className="hero-actions a5">
          <a href="#download" className="btn-gold">
            <Download size={15} />
            Download for Free
          </a>
          <a href="#how-it-works" className="btn-ghost">
            See how it works
            <ChevronRight size={15} />
          </a>
        </div>
      </div>
    </>
  );
}

function Waveform() {
  return (
    <div className="waveform">
      {[1, 2, 3, 4, 5, 6, 7].map((n) => (
        <div key={n} className="wave-bar" />
      ))}
    </div>
  );
}

/* ─── PROOF TICKER ─── */
const TICKER_ITEMS = [
  { label: 'Hymnal', value: 'GHS — 900+ hymns' },
  { label: 'Bible translation', value: 'KJV — all 31,102 verses' },
  { label: 'Detection latency', value: '< 400 ms' },
  { label: 'Platform', value: 'macOS & Windows' },
  { label: 'Price', value: 'Free & open source' },
  { label: 'Setup time', value: 'Under 5 minutes' },
];

function ProofTicker() {
  const items = [...TICKER_ITEMS, ...TICKER_ITEMS];
  return (
    <div className="ticker-wrap" aria-hidden="true">
      <div className="ticker-track">
        {items.map(({ label, value }, i) => (
          <React.Fragment key={i}>
            <div className="ticker-item">
              {label}&nbsp;<strong>{value}</strong>
            </div>
            <div className="ticker-item">
              <div className="ticker-sep" />
            </div>
          </React.Fragment>
        ))}
      </div>
    </div>
  );
}

/* ─── FEATURES ─── */
function Features() {
  return (
    <section className="section" id="features">
      <div className="s-inner">
        <div className="s-head">
          <div className="eyebrow">What It Does</div>
          <h2 className="s-h2">Two things, done perfectly.</h2>
          <p className="s-p">
            Companion Bible does exactly two things and does them without you touching the keyboard.
            Everything else is noise.
          </p>
        </div>

        <div className="feature-trio">
          <FTrioItem
            n="01"
            title="Bible verse detection"
            desc={`Streams your pastor's voice live and detects every verse citation — spoken naturally, in any order, across sentences. Romans 8:28, "chapter eight verse twenty-eight", even "Romans and twenty-eight" — all caught, all displayed.`}
            foot="KJV · All 31,102 verses"
            points={[
              'Spoken numbers understood: "eight twenty-eight"',
              'References split across sentences still detected',
              'Confidence scoring keeps every detection accurate',
            ]}
          />
          <FTrioItem
            n="02"
            title="GHS hymn display"
            desc="The choir leader calls the hymn number. Companion Bible opens it immediately and advances each stanza as the congregation sings — timed automatically, hands completely free."
            foot="GHS Hymnal · 900+ hymns"
            points={[
              'Auto-advances stanza by stanza as they sing',
              'Chorus repeats handled automatically',
              'Service lead can load any hymn manually at any time',
            ]}
          />
          <FTrioItem
            n="03"
            title="Order of service"
            desc="Plan your entire service in advance. The runsheet shows NOW and NEXT on the congregation screen so the service flows — no one shouts page numbers, no one scrambles."
            foot="Service Planning · Dual Window"
            points={[
              'Pre-load hymns and readings before service',
              'NOW/NEXT displayed on congregation screen',
              'Control panel for the service lead on a second display',
            ]}
          />
        </div>
      </div>
    </section>
  );
}

function FTrioItem({
  n,
  title,
  desc,
  foot,
  points,
}: {
  n: string;
  title: string;
  desc: string;
  foot: string;
  points: string[];
}) {
  return (
    <div className="ftrio-item">
      <div className="ftrio-num">{n}.</div>
      <h3 className="ftrio-title">{title}</h3>
      <p className="ftrio-desc">{desc}</p>
      <ul
        style={{
          listStyle: 'none',
          marginTop: 20,
          display: 'flex',
          flexDirection: 'column',
          gap: 10,
        }}
      >
        {points.map((p) => (
          <li
            key={p}
            style={{
              display: 'flex',
              alignItems: 'flex-start',
              gap: 9,
              fontSize: '.82rem',
              color: 'var(--t2)',
              lineHeight: 1.6,
            }}
          >
            <div
              style={{
                width: 18,
                height: 18,
                borderRadius: 5,
                flexShrink: 0,
                marginTop: 1,
                background: 'rgba(34,197,94,.1)',
                border: '1px solid rgba(34,197,94,.18)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <Check size={10} color="#22C55E" strokeWidth={3} />
            </div>
            {p}
          </li>
        ))}
      </ul>
      <div className="ftrio-foot">{foot}</div>
    </div>
  );
}

/* ─── EDITORIAL SHOWCASE ───────────────────────────────
   The single section designed to stop the scroll.
   A scripture verse, perfectly typeset, full-width.
   No boxes. No cards. Just the word.
────────────────────────────────────────────────────── */
function EditorialShowcase() {
  const { ref, visible } = useInView(0.25);

  return (
    <section className="showcase section-dark">
      <div className="showcase-label">What your congregation sees</div>

      <div ref={ref}>
        <blockquote className={`showcase-verse${visible ? ' visible' : ''}`}>
          "For God so loved the world, that he gave his only begotten Son, that whosoever believeth
          in him should not perish, but have <mark>everlasting life.</mark>"
        </blockquote>

        <div className={`showcase-ref${visible ? ' visible' : ''}`}>
          John 3 : 16 · King James Version · Auto-detected
        </div>

        <p className={`showcase-caption${visible ? ' visible' : ''}`}>
          <strong>This is what your congregation sees.</strong> Every verse. Every service.
          Automatically — the moment the pastor speaks it.
        </p>
      </div>
    </section>
  );
}

/* ─── HOW IT WORKS ─── */
function HowItWorks() {
  const steps = [
    {
      n: '01',
      title: 'Plug in a mic',
      desc: `Any microphone near the pulpit \u2014 a lapel, USB condenser, or the laptop\u2019s built-in mic.`,
    },
    {
      n: '02',
      title: 'Open the app',
      desc: 'Connect your congregation screen as a second display and click Start Session.',
    },
    {
      n: '03',
      title: 'Preach normally',
      desc: 'The app streams the sermon live and detects verse citations and hymn numbers from natural speech.',
    },
    {
      n: '04',
      title: 'Scripture appears',
      desc: 'The congregation screen updates in real time, completely hands-free.',
    },
  ];

  return (
    <section className="section" id="how-it-works">
      <div className="s-inner">
        <div className="s-head">
          <div className="eyebrow">Setup</div>
          <h2 className="s-h2">Running in five minutes flat.</h2>
          <p className="s-p">
            No configuration files. No external accounts to create. Works the first time you open
            it.
          </p>
        </div>

        <div className="steps-grid">
          {steps.map(({ n, title, desc }) => (
            <div key={n} className="step">
              <div className="step-num">{n}</div>
              <h3 className="step-h3">{title}</h3>
              <p className="step-p">{desc}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

/* ─── DOWNLOAD ─── */
function DownloadSection() {
  return (
    <section className="dl-section" id="download">
      <div className="dl-inner">
        <div className="dl-platform-label">✦ Free & open source · MIT licence</div>

        <h2 className="dl-h2">
          Download and be ready
          <br />
          for <em>Sunday.</em>
        </h2>
        <p className="dl-sub">
          Available for macOS and Windows. No account. No subscription. Requires macOS 12+ or
          Windows 10+.
        </p>

        <div className="dl-btns">
          <a
            className="btn-dl"
            href="https://github.com/Projea12/companion-bible/releases/latest/download/companion-bible-mac.dmg"
            download="companion-bible-mac.dmg"
            rel="noopener noreferrer"
          >
            <div className="btn-dl-icon">
              <AppleIcon />
            </div>
            <div className="btn-dl-text">
              <strong>Download for macOS</strong>
              <span>macOS 12+ · Apple Silicon & Intel</span>
            </div>
          </a>

          <a
            className="btn-dl"
            href="https://github.com/Projea12/companion-bible/releases/latest/download/companion-bible-windows.exe"
            download="companion-bible-windows.exe"
            rel="noopener noreferrer"
          >
            <div className="btn-dl-icon">
              <WindowsIcon />
            </div>
            <div className="btn-dl-text">
              <strong>Download for Windows</strong>
              <span>Windows 10/11 · ~6 MB installer</span>
            </div>
          </a>
        </div>

        <p className="dl-note">
          All releases on{' '}
          <a href="https://github.com/Projea12/companion-bible/releases">GitHub Releases</a>. Source
          code open under the MIT licence.
        </p>
      </div>
    </section>
  );
}

/* ─── FOOTER ─── */
function Footer() {
  return (
    <footer className="footer">
      <div className="footer-inner">
        <div className="footer-logo">
          <div className="logomark" style={{ width: 26, height: 26, borderRadius: 7 }}>
            <BookOpen size={13} color="#fff" />
          </div>
          Companion Bible
        </div>
        <p className="footer-copy">© 2026 Companion Bible. Open source, free forever.</p>
        <ul className="footer-links">
          <li>
            <a href="https://github.com/johnolugbemi/companion-bible">GitHub</a>
          </li>
          <li>
            <a href="#features">Features</a>
          </li>
          <li>
            <a href="#download">Download</a>
          </li>
        </ul>
      </div>
    </footer>
  );
}

/* ─── ICONS ─── */
function AppleIcon() {
  return (
    <svg width="20" height="20" viewBox="0 0 814 1000" fill="var(--gold)">
      <path d="M788.1 340.9c-5.8 4.5-108.2 62.2-108.2 190.5 0 148.4 130.3 200.9 134.2 202.2-.6 3.2-20.7 71.9-68.7 141.9-42.8 61.6-87.5 123.1-155.5 123.1s-85.5-39.5-164-39.5c-76 0-103.7 40.8-165.9 40.8s-105-57.8-155.5-127C46.7 790.7 0 663 0 541.8c0-207.5 135.4-317.3 268.3-317.3 99.6 0 182.4 65.7 244.8 65.7 60.1 0 154.5-69.7 268.1-69.7 43.4 0 150.4 4 214.3 107.4zm-261.3-189.5c59.1-70.4 101.2-168.3 101.2-266.2 0-13.5-1.3-27.1-3.9-38.3C542.8 5.7 418.3 74.1 348.7 151.5c-53.9 60.9-105.3 160.5-105.3 256.7 0 15.6 2.6 31.2 3.9 36.1 6.5 1.3 17.2 2.6 27.9 2.6 96.8 0 208.8-65.1 263-95.5z" />
    </svg>
  );
}

function WindowsIcon() {
  return (
    <svg width="20" height="20" viewBox="0 0 88 88" fill="var(--gold)">
      <path d="M0 12.4 35.7 7.6l.1 34.4-35.7.2zm35.8 33.5.1 34.4-35.8-5v-29.2zm4.3-39.2 47.6-6.7v41.2l-47.6.4zm47.7 43.4-.1 40.8-47.6-6.6-.1-34.6z" />
    </svg>
  );
}
