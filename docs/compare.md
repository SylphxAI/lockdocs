# Comparison

| | **lockdocs** | Context7 | docs-mcp-server | Ref |
|---|---|---|---|---|
| Where docs come from | Your installed packages | Hosted index of public repos and sites | Sites and repos you scrape into a local index | Hosted index |
| Which version | Exact, from your lockfile | Chosen by library ID or prompt; a few indexed versions per library | The version you scraped | Mostly latest |
| Per-library setup | None | None | Scrape each library (and version) | None |
| Private / internal packages | Yes, if installed | Paid plans | Yes, if you scrape them | Private GitHub repos and PDFs you connect (plan limits) |
| API reference from types and doc comments | `.d.ts`, docstrings, rustdoc, Go doc | Doc snippets | Doc pages | Doc pages |
| Works offline | Yes | No | After scraping | No |
| Limits | None | Anonymous `ratelimit-limit: 200` (observed); free key 1,000 calls/month | None | 200 free credits once, then from $19/month |
| Account / API key | No | Optional | Optional (embeddings) | Yes |
| License | MIT | MIT client, hosted service | MIT | Proprietary |

Context7 knows many sites and guides that never ship inside a package, so for a question whose answer lives only on a docs website, it can do better. lockdocs is for the other case: when the version matters, when you are offline or rate-limited, and when the package is yours. They combine well.

## When to use which

- **lockdocs**: the version matters (a major upgrade, a pinned older release), you are offline or rate-limited, the package is private or internal, or you want the exact signature from the installed types.
- **Context7 / Ref**: the answer lives in a guide on a docs website that is not shipped inside the package, or the library is not installed.
- **docs-mcp-server**: you want a local semantic index of specific websites and are happy to scrape each one.

Running lockdocs next to a hosted doc server is a good setup: lockdocs answers "what does *my* version do", the hosted one fills in website-only guides.
