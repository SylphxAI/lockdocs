use super::docs::{page_markdown, page_title, prose_titled};
use super::go::go_file;
use super::package::legacy_file;
use super::python::py_file;
use super::rust::rs_file;
use super::typescript::ts_file;
use super::{Entry, Kind};

fn run(f: fn(&str, &str, &mut Vec<Entry>), text: &str, rel: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    f(text, rel, &mut out);
    out
}

#[test]
fn typescript_declarations() {
    let src = r#"
/** Creates an object schema. */
export declare function object<T extends ZodRawShape>(shape: T, params?: string): ZodObject<T>;
export declare namespace z {
  /** A string schema. */
  function string(): ZodString;
}
export interface ZodType<T> {
  /** Parse or throw. */
  parse(data: unknown): T;
  readonly _def: Def;
}
export declare class ZodObject<T> extends ZodType<T> {
  /** Disallow unknown keys. */
  strict(message?: string): ZodObject<T>;
  private _cached;
}
declare const objectType: <T>(shape: T) => ZodObject<T>;
export { objectType as obj };
export * as zz from "./external";
"#;
    let e = run(ts_file, src, "lib/index.d.ts");
    let get = |p: &str| {
        e.iter()
            .find(|x| x.path == p)
            .unwrap_or_else(|| panic!("missing {p}: {:#?}", e.iter().map(|x| &x.path).collect::<Vec<_>>()))
    };
    assert_eq!(get("object").doc, "Creates an object schema.");
    assert!(get("object").sig.contains("ZodObject<T>"));
    assert_eq!(get("z.string").doc, "A string schema.");
    assert_eq!(get("ZodType.parse").kind, Kind::Method);
    assert!(e.iter().all(|x| x.name != "_def" && x.name != "_cached"));
    assert_eq!(get("ZodObject.strict").doc, "Disallow unknown keys.");
    assert_eq!(get("objectType").kind, Kind::Function);
    assert_eq!(get("obj").alias_of.as_deref(), Some("objectType"));
    assert_eq!(get("zz").kind, Kind::Module);
    assert_eq!(get("object").line, 3);
}

#[test]
fn legacy_dirs() {
    assert!(legacy_file("v3/types.d.ts", 4));
    assert!(!legacy_file("v4/classic/schemas.d.ts", 4));
    assert!(legacy_file("pydantic/v1/main.py", 2));
    assert!(!legacy_file("pydantic/main.py", 2));
    assert!(!legacy_file("src/v2/x.rs", 1));
}

#[test]
fn python_definitions() {
    let src = "\"\"\"Module docs.\"\"\"\n\nclass BaseModel:\n    \"\"\"Base class.\n\n    More.\n    \"\"\"\n\n    def model_dump(self, *, mode: str = 'python') -> dict:\n        \"\"\"Dump the model.\"\"\"\n        return {}\n\n    @classmethod\n    def model_validate(cls, obj):\n        '''Validate.'''\n\n    def _private(self): pass\n\ndef create_model(name: str, **fields) -> type:\n    return None\n";
    let e = run(py_file, src, "pydantic/main.py");
    let get = |p: &str| e.iter().find(|x| x.path == p).unwrap_or_else(|| panic!("missing {p}"));
    assert_eq!(get("pydantic.main.BaseModel").doc, "Base class.\n\nMore.");
    assert_eq!(
        get("pydantic.main.BaseModel.model_dump").sig,
        "def model_dump(self, *, mode: str = 'python') -> dict"
    );
    assert!(get("pydantic.main.BaseModel.model_validate").sig.starts_with("@classmethod def model_validate"));
    assert!(e.iter().all(|x| x.name != "_private"));
    assert!(e.iter().any(|x| x.kind == Kind::Prose && x.doc == "Module docs."));
    assert_eq!(get("pydantic.main.create_model").kind, Kind::Function);
}

#[test]
fn docs_pages_written_as_components() {
    let src = r#"import dedent from "dedent";
export const metadata = { title: "Installing with Vite", description: "Use the Vite plugin.", openGraph: { title: "x" } };
const steps = [
  {
    title: "Import Tailwind CSS",
    body: (
      <p>
        Add an <code>@import</code> to your CSS file.
      </p>
    ),
    code: { name: "CSS", lang: "css", code: dedent`
        @import "tailwindcss";
      ` },
  },
  { title: 'Old way', body: () => <p>Use directives{' '}here.</p>, code: { lang: 'css', code: '@tailwind base;\n@tailwind utilities;' } },
];
export default function Page() { return <div className="prose"><h3 className="sr-only">Overview</h3><p>Fast.</p></div> }
"#;
    let (md, title) = page_markdown(src).unwrap();
    assert_eq!(title.as_deref(), Some("Installing with Vite"));
    assert!(md.contains("## Import Tailwind CSS"), "{md}");
    assert!(md.contains("Add an `@import` to your CSS file."), "{md}");
    assert!(md.contains("```css\n@import \"tailwindcss\";\n```"), "{md}");
    assert!(md.contains("@tailwind base;\n@tailwind utilities;"), "{md}");
    assert!(md.contains("### Overview"), "{md}");
    assert!(!md.contains("dedent") && !md.contains("prose"), "{md}");
    let mut out = Vec::new();
    prose_titled(&md, "upstream:site/docs/installation/page.tsx", 0, title, &mut out);
    let s = out.iter().find(|e| e.name == "Installing with Vite › Import Tailwind CSS").unwrap();
    assert_eq!(s.line, 5);
    assert_eq!(
        page_title("upstream:site/src/app/(docs)/docs/installation/(tabs)/using-vite/page.tsx"),
        "installation/using-vite"
    );
}

#[test]
fn rust_items_and_macro_bodies() {
    let src = "//! Crate docs.\n//!\n//! # Usage\n//! Call spawn.\n\ncfg_rt! {\n    /// Spawns a new task.\n    #[track_caller]\n    pub fn spawn<F>(future: F) -> JoinHandle<F::Output>\n    where F: Future + Send + 'static {\n        todo!()\n    }\n}\n\n/// A router.\npub struct Router<S = ()> { inner: Inner }\n\nimpl<S> Router<S> {\n    /// Add a route.\n    pub fn route(self, path: &str, m: MethodRouter<S>) -> Self { self }\n    fn private(&self) {}\n}\n\n/// Hidden\n#[doc(hidden)]\npub fn hidden() {}\n\n/// Macro.\n#[macro_export]\nmacro_rules! select { () => {} }\n";
    let mut e = Vec::new();
    rs_file(src, "src/task/spawn.rs", "tokio", &mut e);
    let get = |p: &str| {
        e.iter()
            .find(|x| x.path == p)
            .unwrap_or_else(|| panic!("missing {p}: {:#?}", e.iter().map(|x| &x.path).collect::<Vec<_>>()))
    };
    let spawn = get("tokio::task::spawn::spawn");
    assert_eq!(spawn.doc, "Spawns a new task.");
    assert_eq!(spawn.line, 9);
    assert!(spawn.sig.contains("JoinHandle"));
    assert_eq!(get("tokio::task::spawn::Router::route").doc, "Add a route.");
    assert!(e.iter().all(|x| x.name != "private" && x.name != "hidden"));
    assert_eq!(get("tokio::select!").kind, Kind::Macro);
    assert!(e.iter().any(|x| x.kind == Kind::Prose && x.path.ends_with("Usage")));

    // tokio's doc-wrapper pattern
    let src = "macro_rules! doc {\n    ($select:item) => {\n        /// Waits on multiple branches.\n        #[macro_export]\n        $select\n    };\n}\n\ndoc! {macro_rules! select {\n    () => {};\n}}\n";
    let mut e = Vec::new();
    rs_file(src, "src/macros/select.rs", "tokio", &mut e);
    let sel = e.iter().find(|x| x.path == "tokio::select!").expect("select! extracted");
    assert_eq!(sel.doc, "Waits on multiple branches.");
    assert!(e.iter().all(|x| x.name != "doc"));
}

#[test]
fn go_declarations() {
    let src = "// Package gin implements a web framework.\npackage gin\n\n// Context is the request context.\ntype Context struct {\n\tKeys map[string]any\n}\n\n// JSON serializes obj.\nfunc (c *Context) JSON(code int, obj any) {\n}\n\n// Default returns an Engine.\nfunc Default(opts ...Option) *Engine {\n\treturn nil\n}\n\nfunc internal() {}\n\ntype Handler interface {\n\t// Serve serves.\n\tServe(c *Context)\n}\n";
    let mut e = Vec::new();
    go_file(src, "context.go", "github.com/gin-gonic/gin", &mut e);
    let get = |p: &str| {
        e.iter()
            .find(|x| x.path == p)
            .unwrap_or_else(|| panic!("missing {p}: {:#?}", e.iter().map(|x| &x.path).collect::<Vec<_>>()))
    };
    assert_eq!(get("gin.Context.JSON").doc, "JSON serializes obj.");
    assert_eq!(get("gin.Default").sig, "func Default(opts ...Option) *Engine");
    assert_eq!(get("gin.Context").doc, "Context is the request context.");
    assert_eq!(get("gin.Handler.Serve").doc, "Serve serves.");
    assert!(e.iter().all(|x| x.name != "internal"));
    assert!(e.iter().any(|x| x.kind == Kind::Prose && x.doc.contains("web framework")));
}
