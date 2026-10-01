# Benchmarks

## Method

- **Questions:** 105 questions whose correct answer depends on the version, in [`bench/questions.json`](https://github.com/SylphxAI/lockdocs/blob/main/bench/questions.json), over zod 3/4, Next.js 14/15/16, React Router 6/7, pydantic 1/2, axum 0.7/0.8, tokio, Tailwind CSS 3/4, ESLint 8/9, Prisma 5/6, React 18/19, Vite 5/6, Express 4/5, SQLAlchemy 1.4/2.0, Django 4.2/5.1 and FastAPI 0.88/0.115. Twin questions are worded identically for both versions; each records its source of truth.
- **Held-out questions:** 18 questions marked `held-out` are not used for tuning: they were written before the ranking changes they measure and have their own column. Held-out questions that are later used to diagnose a miss join the main set (their `history` field says so), and new ones replace them.
- **Projects:** one project per version in [`bench/projects`](https://github.com/SylphxAI/lockdocs/tree/main/bench/projects), installed at those pins by [`bench/setup.sh`](https://github.com/SylphxAI/lockdocs/blob/main/bench/setup.sh).
- **Grading:** an answer passes when it contains at least one string from every `expect` group (the version-correct API, in code or prose form) and none of the `reject` strings (the other version's API). Case-insensitive; the same grader for every tool.
- **lockdocs**, default settings (1,200-token budget), in four configurations: keyword only (`LOCKDOCS_EMBED=0`) on package files; hybrid on package files (`LOCKDOCS_FETCH=0`, upstream disabled); real first-use defaults in an initially empty isolated `LOCKDOCS_CACHE` with fetch-policy and credential variables removed, before explicit prefetch; hybrid after explicit `lockdocs fetch` added upstream docs and major-version sites. The first-use row includes cold-download latency and anonymous rate-limit failures; it is never copied from the prefetched score. Full runs enforce at least 96/105 prefetched and 60/105 package-only hybrid. Index and fetch times are reported separately.
- **Context7:** the anonymous API as its MCP server uses it: search the library, pick the top result and its listed version with the same major (exact when listed), then fetch context for the question. When that library answers HTTP 404, the next search result is tried, as an agent would. Rate-limit responses are recorded, not retried.
- **Tokens:** tiktoken `o200k_base`. **Latency:** wall time per call from the same GitHub-hosted runner (for lockdocs: a fresh CLI process per question, including loading the model).
- **Runner:** [`bench/run.py`](https://github.com/SylphxAI/lockdocs/blob/main/bench/run.py) via the [`bench` workflow](https://github.com/SylphxAI/lockdocs/actions/workflows/bench.yml). Reproduce: `bash bench/setup.sh && python3 bench/run.py target/release/lockdocs bench/projects out.json --fetch --context7`.

## Results

<!-- results -->

From [this run](https://github.com/SylphxAI/lockdocs/actions/runs/36804076600), on the lockdocs 0.4.0 release branch (commit b05aecd).

Tokenizer: tiktoken o200k_base. Runner: Linux x86_64. 105 questions.

| | correct | older major | newer major | single version | held-out | median tokens | median latency | p95 latency |
|---|---|---|---|---|---|---|---|---|
| lockdocs, real first-use defaults (empty isolated cache, anonymous release-tag fetch) | 70/105 | 28/45 | 38/55 | 4/5 | 10/18 | 894 | 185 ms | 2258 ms |
| lockdocs, hybrid + upstream docs (after `lockdocs fetch`) | 96/105 | 38/45 | 53/55 | 5/5 | 16/18 | 880 | 58 ms | 313 ms |
| Context7 (anonymous API) | 77/105 | 19/45 | 53/55 | 5/5 | 15/18 | 908 | 2583 ms | 3398 ms |

Context7 (anonymous): answers reused from earlier runs (count in the run artifact).

The per-question table, index build times and `lockdocs fetch` times are in the run's job summary and its `bench` artifact.

## Reading the results

- **lockdocs is ahead overall (96/105 vs 77/105) and on older majors by 2x (38/45 vs 19/45), ties Context7 on the newest majors (53/55 each) and on tokio (5/5 each), and is ahead on the held-out questions (16/18 vs 15/18).** It also uses fewer tokens (880 vs 908 median) and answers in 58 ms instead of 2,583 ms (Context7 latency from the earlier runs its reused answers were measured in).
- **Default first use: 70/105.** From an empty cache with no GitHub token, the first query downloads release-tag docs anonymously; median 185 ms including those downloads (p95 2258 ms). Running `lockdocs fetch` once raises it to 96/105.
- **Where each still misses.** lockdocs: seven older-major questions (among them Next.js 14 `cookies()`, React Router 6 `json()`, FastAPI 0.88 `on_event`, Prisma 5 `Buffer`, ESLint 8 `env`), and two held-out newer-major questions (ESLint 9 plugins, Tailwind 4 `@source`). Context7 mostly answers older-major questions with the newest API, and two of its Tailwind answers came from unrelated libraries its search ranked first.
- **Configurations matter (from run 36204404336).** Keyword-only on package files: 59/105. Adding the local embedding model: 60/105. Adding `lockdocs fetch` (upstream docs at each version's git tag, plus docs-site repositories for your major): 96/105.
- Latency for lockdocs is a fresh CLI process per question, including loading the embedding model; the MCP server keeps it loaded.
- Context7 answers for questions whose wording, grading and pinned version are unchanged since the previous run were reused from that run (disclosed in the results line) to stay within the anonymous quota.
- Contributions of new version-sensitive questions are welcome.
