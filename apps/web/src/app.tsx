import type { ParentProps } from "solid-js";
import { Router } from "@solidjs/router";
import { FileRoutes } from "@solidjs/start/router";
import { MetaProvider } from "@solidjs/meta";
import { A as Link } from "@solidjs/router";
import { ArrowUpRight, Braces, GitFork } from 'lucide-solid';
import '@fontsource-variable/inter';
import '@fontsource-variable/jetbrains-mono';
import './globals.css';


const github = 'https://github.com/SylphxAI/lockdocs';
function Wordmark() {
  return <Link href="/" class="wordmark"><img src="/brand/lockdocs.svg" alt="" width="31" height="31"/><span>lockdocs<span class="wordmark-dot">.</span></span></Link>;
}
function Layout(props: ParentProps) {
  return <>
    <a class="skip" href="#main">Skip to content</a>
    <header class="site-header"><div class="wrap header-inner">
      <Wordmark />
      <nav aria-label="Main navigation" class="main-nav"><Link href="/docs">Docs</Link><Link href="/playground">Playground</Link><Link href="/pricing">Pricing</Link></nav>
      <div class="header-tools"><a class="repo-link" href={github} aria-label="Lockdocs on GitHub"><GitFork size={17}/></a><Link href="/dashboard" class="account-link">Account <ArrowUpRight size={13}/></Link></div>
    </div></header>
    <main id="main">{props.children}</main>
    <footer class="site-footer"><div class="wrap footer-inner"><Wordmark/><p>Exact-version docs, by Sylphx.</p><nav aria-label="Footer navigation"><Link href="/benchmarks">Benchmarks</Link><Link href="/compare">Compare</Link><a href={github}><Braces size={14}/> Source</a></nav></div></footer>
  </>;
}

export default function App() {
 return <Router root={props => <MetaProvider><Layout>{props.children}</Layout></MetaProvider>}><FileRoutes /></Router>;
}
