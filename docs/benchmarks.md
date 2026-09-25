# Benchmarks

## Method

- **Questions:** 36 questions whose correct answer depends on the version, in [`bench/questions.json`](https://github.com/SylphxAI/lockdocs/blob/main/bench/questions.json): zod 3.23.8 vs 4.1.5, Next.js 14.2.15 vs 15.1.0, React Router 6.26.2 vs 7.1.1, pydantic 1.10.18 vs 2.9.2, axum 0.7.9 vs 0.8.1, and tokio.
- **Projects:** one project per version in [`bench/projects`](https://github.com/SylphxAI/lockdocs/tree/main/bench/projects), installed at those pins by [`bench/setup.sh`](https://github.com/SylphxAI/lockdocs/blob/main/bench/setup.sh) (`npm install`, a Python 3.12 virtualenv, `cargo fetch`).
- **Grading:** an answer passes when it contains at least one string from every `expect` group (the version-correct API, e.g. `model_dump` for pydantic 2, `.dict(` for pydantic 1) and none of the `reject` strings (the other version's API). Matching is case-insensitive. The same grader runs on every tool.
- **lockdocs:** `lockdocs docs "<question>" --pkg <package>` in the project, default 2,000-token budget, cache cleared before the run (index time is reported separately).
- **Context7:** the anonymous API as its MCP server uses it: search the library, pick the top result and its listed version with the same major (exact when listed), then fetch context for the question. Rate-limit responses are recorded, not retried.
- **Tokens:** tiktoken `o200k_base`. **Latency:** wall time per call, from the same GitHub-hosted runner.
- **Runner:** [`bench/run.py`](https://github.com/SylphxAI/lockdocs/blob/main/bench/run.py) via the [`bench` workflow](https://github.com/SylphxAI/lockdocs/actions/workflows/bench.yml). Run it yourself: `bash bench/setup.sh && python3 bench/run.py target/release/lockdocs bench/projects out.json --context7`.

## Results

<!-- results -->

## Reading the results

- lockdocs answers only from what the package ships. When a library keeps its guides on a website only (Next.js ships no docs in its npm package), lockdocs relies on type declarations and doc comments, and a hosted index can know more.
- Context7 serves a few indexed versions per library; when the pinned version is not one of them, the table says which one it used.
- The question set is small and fixed on purpose, so anyone can rerun it and check every answer. Contributions of new version-sensitive questions are welcome.
