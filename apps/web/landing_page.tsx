import { BookOpen, Mic, Monitor, Music, Download, Zap, WifiOff } from 'lucide-react';

const LandingPage = () => {
  return (
    <div
      style={{
        minHeight: '100vh',
        backgroundColor: '#0a0a0a',
        fontFamily: 'system-ui, sans-serif',
        color: '#fff',
      }}
    >
      {/* Navbar */}
      <nav
        style={{
          padding: '1.25rem 2rem',
          backgroundColor: '#0a0a0a',
          borderBottom: '1px solid #1a1a1a',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          position: 'sticky',
          top: 0,
          zIndex: 100,
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.6rem' }}>
          <BookOpen size={22} color="#646cff" />
          <span style={{ fontWeight: '700', fontSize: '1.1rem' }}>Companion Bible</span>
        </div>
        <a
          href="#download"
          style={{
            padding: '0.6rem 1.4rem',
            backgroundColor: '#646cff',
            color: '#fff',
            borderRadius: '8px',
            fontSize: '0.9rem',
            fontWeight: '600',
            textDecoration: 'none',
          }}
        >
          Download Free
        </a>
      </nav>

      {/* Hero */}
      <section
        style={{
          padding: '7rem 2rem',
          background: 'linear-gradient(135deg, #0a0a0a 0%, #1a0a2e 100%)',
          textAlign: 'center',
        }}
      >
        <div style={{ maxWidth: '860px', margin: '0 auto' }}>
          <div
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: '0.5rem',
              padding: '0.5rem 1rem',
              backgroundColor: '#1a1a1a',
              borderRadius: '50px',
              marginBottom: '2rem',
              border: '1px solid #333',
            }}
          >
            <Zap size={15} color="#646cff" />
            <span style={{ fontSize: '0.85rem', color: '#888' }}>
              Real-time scripture &amp; hymn display for live services
            </span>
          </div>

          <h1
            style={{
              fontSize: 'clamp(2.4rem, 5vw, 3.8rem)',
              fontWeight: 'bold',
              lineHeight: 1.15,
              marginBottom: '1.5rem',
              background: 'linear-gradient(135deg, #fff 0%, #646cff 100%)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
              backgroundClip: 'text',
            }}
          >
            Scripture &amp; Hymns,
            <br />
            Live on the Congregation Screen
          </h1>

          <p
            style={{
              fontSize: '1.2rem',
              color: '#aaa',
              maxWidth: '640px',
              margin: '0 auto 1rem',
              lineHeight: '1.8',
            }}
          >
            Companion Bible listens to the preacher, detects Bible verse citations in real time, and
            automatically displays the full KJV text on a second screen — no operator action
            required.
          </p>
          <p
            style={{
              fontSize: '1.05rem',
              color: '#888',
              maxWidth: '560px',
              margin: '0 auto 3rem',
              lineHeight: '1.7',
            }}
          >
            Also detects GHS hymn numbers from speech and advances stanzas and choruses
            automatically as the congregation sings.
          </p>

          <div style={{ display: 'flex', gap: '1rem', justifyContent: 'center', flexWrap: 'wrap' }}>
            <a
              href="#download"
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: '0.5rem',
                padding: '1rem 2rem',
                backgroundColor: '#646cff',
                color: '#fff',
                borderRadius: '8px',
                fontSize: '1.05rem',
                fontWeight: '600',
                textDecoration: 'none',
              }}
            >
              <Download size={18} />
              Download for Mac or Windows
            </a>
            {/* <a
              href="https://github.com/Projea12/companion-bible"
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: '0.5rem',
                padding: '1rem 2rem',
                backgroundColor: '#1a1a1a',
                color: '#fff',
                borderRadius: '8px',
                fontSize: '1.05rem',
                fontWeight: '600',
                border: '1px solid #333',
                textDecoration: 'none',
              }}
            >
              View on GitHub
              <ArrowRight size={18} />
            </a> */}
          </div>
        </div>
      </section>

      {/* What It Does */}
      <section style={{ padding: '6rem 2rem', backgroundColor: '#0f0f0f' }}>
        <div style={{ maxWidth: '1100px', margin: '0 auto' }}>
          <h2
            style={{ fontSize: '2.2rem', textAlign: 'center', marginBottom: '1rem', color: '#fff' }}
          >
            What Companion Bible Does
          </h2>
          <p
            style={{
              textAlign: 'center',
              color: '#888',
              marginBottom: '4rem',
              fontSize: '1.05rem',
            }}
          >
            Two powerful features for live church services — all running on a single laptop.
          </p>

          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))',
              gap: '2rem',
            }}
          >
            <FeatureCard
              icon={<BookOpen size={38} color="#646cff" />}
              title="Automatic Bible Verse Display"
              description={`The pastor says "Romans eight twenty-eight" — within ~400 ms the congregation screen shows the full KJV verse. No button press needed. Pattern matching, local AI, and cloud AI work together to catch every citation.`}
            />
            <FeatureCard
              icon={<Music size={38} color="#646cff" />}
              title="GHS Hymn Display"
              description={`Say "open GHS two hundred and thirty four" and the first stanza appears instantly. The display auto-advances to the next stanza or chorus as the congregation sings, keeping pace without operator input.`}
            />
          </div>
        </div>
      </section>

      {/* How It Works */}
      <section style={{ padding: '6rem 2rem' }}>
        <div style={{ maxWidth: '1100px', margin: '0 auto' }}>
          <h2
            style={{ fontSize: '2.2rem', textAlign: 'center', marginBottom: '4rem', color: '#fff' }}
          >
            How It Works
          </h2>

          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))',
              gap: '3rem',
            }}
          >
            <ProcessStep
              number="1"
              title="Plug In a Mic"
              description="Connect a microphone near the pulpit or choir leader. A lapel, USB, or built-in mic all work."
            />
            <ProcessStep
              number="2"
              title="Start a Session"
              description="Open Companion Bible, click Start Session, and connect a second monitor for the congregation display."
            />
            <ProcessStep
              number="3"
              title="Preach Normally"
              description="The AI listens continuously. Bible references and GHS hymn numbers are detected automatically from natural speech."
            />
            <ProcessStep
              number="4"
              title="Scripture Appears"
              description="The congregation screen updates in real time. The operator can confirm, override, or manually load content at any time."
            />
          </div>
        </div>
      </section>

      {/* Features */}
      <section style={{ padding: '6rem 2rem', backgroundColor: '#0f0f0f' }}>
        <div style={{ maxWidth: '1100px', margin: '0 auto' }}>
          <h2
            style={{ fontSize: '2.2rem', textAlign: 'center', marginBottom: '4rem', color: '#fff' }}
          >
            Built for African Churches
          </h2>

          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))',
              gap: '2rem',
            }}
          >
            <FeatureCard
              icon={<Mic size={38} color="#646cff" />}
              title="Optimised for African Accents"
              description="AssemblyAI streaming transcription with accent-aware patterns. Handles spoken numbers, 'and' as verse separator, and references fragmented across sentences."
            />
            <FeatureCard
              icon={<WifiOff size={38} color="#646cff" />}
              title="Works Fully Offline"
              description="No internet? No problem. Whisper local transcription + Phi-3 Mini on-device AI keeps detection running. API keys are optional, not required."
            />
            <FeatureCard
              icon={<Monitor size={38} color="#646cff" />}
              title="Dual-Window Display"
              description="An operator window gives full control: confirm or override detections, manually load any verse or hymn, toggle Bible/GHS mode, advance stanzas."
            />
          </div>
        </div>
      </section>

      {/* Download */}
      <section
        id="download"
        style={{
          padding: '6rem 2rem',
          background: 'linear-gradient(135deg, #1a0a2e 0%, #0a0a0a 100%)',
          textAlign: 'center',
        }}
      >
        <div style={{ maxWidth: '700px', margin: '0 auto' }}>
          <h2 style={{ fontSize: '2.4rem', marginBottom: '1rem', color: '#fff' }}>
            Download Companion Bible
          </h2>
          <p style={{ fontSize: '1.1rem', color: '#aaa', marginBottom: '3rem', lineHeight: '1.7' }}>
            Free and open source. Available for macOS and Windows. Requires macOS 12+ or Windows
            10+.
          </p>

          <div
            style={{ display: 'flex', gap: '1.5rem', justifyContent: 'center', flexWrap: 'wrap' }}
          >
            <PlatformButton
              label="Download for macOS"
              sublabel="macOS 12+ · Apple Silicon &amp; Intel"
              href="https://github.com/johnolugbemi/companion-bible/releases/latest/download/companion-bible-mac.dmg"
            />
            <PlatformButton
              label="Download for Windows"
              sublabel="Windows 10/11 · 64-bit"
              href="https://github.com/johnolugbemi/companion-bible/releases/latest/download/companion-bible-windows.exe"
            />
          </div>

          <p style={{ marginTop: '2rem', fontSize: '0.9rem', color: '#555' }}>
            All releases on{' '}
            <a
              href="https://github.com/johnolugbemi/companion-bible/releases"
              style={{ color: '#646cff', textDecoration: 'none' }}
            >
              GitHub Releases
            </a>
            . Source code available under the MIT licence.
          </p>
        </div>
      </section>

      {/* Footer */}
      <footer
        style={{
          padding: '2rem',
          backgroundColor: '#0f0f0f',
          borderTop: '1px solid #1a1a1a',
          textAlign: 'center',
          color: '#555',
          fontSize: '0.9rem',
        }}
      >
        <div
          style={{
            display: 'flex',
            justifyContent: 'center',
            alignItems: 'center',
            gap: '0.5rem',
            marginBottom: '0.5rem',
          }}
        >
          <BookOpen size={16} color="#646cff" />
          <span style={{ fontWeight: '600', color: '#888' }}>Companion Bible</span>
        </div>
        <p style={{ marginTop: '0.25rem' }}>
          &copy; 2026 Companion Bible. Open source, free forever.
        </p>
      </footer>
    </div>
  );
};

const FeatureCard = ({
  icon,
  title,
  description,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
}) => (
  <div
    style={{
      padding: '2rem',
      backgroundColor: '#1a1a1a',
      borderRadius: '12px',
      border: '1px solid #222',
    }}
  >
    <div style={{ marginBottom: '1rem' }}>{icon}</div>
    <h3 style={{ fontSize: '1.3rem', marginBottom: '0.75rem', color: '#fff' }}>{title}</h3>
    <p style={{ color: '#aaa', lineHeight: '1.7', fontSize: '0.97rem' }}>{description}</p>
  </div>
);

const ProcessStep = ({
  number,
  title,
  description,
}: {
  number: string;
  title: string;
  description: string;
}) => (
  <div style={{ textAlign: 'center' }}>
    <div
      style={{
        width: '58px',
        height: '58px',
        borderRadius: '50%',
        backgroundColor: '#646cff',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        fontSize: '1.4rem',
        fontWeight: 'bold',
        margin: '0 auto 1rem',
      }}
    >
      {number}
    </div>
    <h3 style={{ fontSize: '1.15rem', marginBottom: '0.5rem', color: '#fff' }}>{title}</h3>
    <p style={{ color: '#aaa', lineHeight: '1.6', fontSize: '0.95rem' }}>{description}</p>
  </div>
);

const PlatformButton = ({
  label,
  sublabel,
  href,
}: {
  label: string;
  sublabel: string;
  href: string;
}) => (
  <a
    href={href}
    style={{
      display: 'inline-flex',
      flexDirection: 'column',
      alignItems: 'center',
      gap: '0.35rem',
      padding: '1.1rem 2.2rem',
      backgroundColor: '#646cff',
      color: '#fff',
      borderRadius: '10px',
      textDecoration: 'none',
      minWidth: '230px',
    }}
  >
    <span
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: '0.5rem',
        fontSize: '1.05rem',
        fontWeight: '700',
      }}
    >
      <Download size={18} />
      {label}
    </span>
    <span
      style={{ fontSize: '0.8rem', opacity: 0.75 }}
      dangerouslySetInnerHTML={{ __html: sublabel }}
    />
  </a>
);

export default LandingPage;
