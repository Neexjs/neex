import { Button } from "@{{projectName}}/ui";

export default function Home() {
  return (
    <main className="container">
      <div className="hero">
        <h1 className="title">
          Welcome to <span className="gradient">{{projectName}}</span>
        </h1>
        <p className="subtitle">
          Built with <strong>Neex</strong> - Ultra-fast Monorepo Build Tool
        </p>

        <div className="stack">
          <span className="badge">Next.js 15</span>
          <span className="badge">Hono</span>
          <span className="badge">TypeScript</span>
          <span className="badge">Bun</span>
        </div>

        <div className="actions">
          <Button variant="primary">Get Started</Button>
          <Button variant="secondary">Documentation</Button>
        </div>
      </div>

      <section className="features">
        <div className="feature">
          <span className="icon">⚡</span>
          <h3>Fast Dev</h3>
          <p>Run <code>neex dev --all</code> for instant HMR</p>
        </div>
        <div className="feature">
          <span className="icon">📦</span>
          <h3>Monorepo</h3>
          <p>Shared packages with workspace support</p>
        </div>
        <div className="feature">
          <span className="icon">🔄</span>
          <h3>Full-Stack</h3>
          <p>Next.js frontend + Hono API backend</p>
        </div>
      </section>
    </main>
  );
}
