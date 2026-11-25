use super::*;
use std::collections::HashSet;

#[test]
fn toml_package_arrays_preserve_origins_and_direct_dependencies() {
    let cargo = r#"version = 4
[[package]]
name = "workspace"
version = "0.1.0"
dependencies = ["registry-lib 1.2.3", "git-lib"]
[[package]]
name = "registry-lib"
version = "1.2.3"
source = "sparse+https://index.crates.io/"
[[package]]
name = "git-lib"
version = "2.0.0"
source = "git+https://github.com/example/lib#01234567"
[[package]]
name = "transitive"
version = "3.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
[[package]]
name = "local"
version = "0.1.0"
source = "path+file:///local"
"#;
    assert_eq!(
        cargo_lock(cargo).unwrap(),
        vec![
            dep(Eco::Cargo, "registry-lib", "1.2.3", true, "Cargo.lock"),
            dep(Eco::Cargo, "git-lib", "2.0.0", true, "Cargo.lock (git)"),
            dep(Eco::Cargo, "transitive", "3.0.0", false, "Cargo.lock"),
        ]
    );
    let uv = r#"version = 1
[[package]]
name = "workspace"
version = "0.1.0"
source = { virtual = "." }
dependencies = [{ name = "Registry_Lib" }]
[package.optional-dependencies]
extra = [{ name = "git-lib" }]
[[package]]
name = "editable"
version = "0.1.0"
source = { editable = "editable" }
[package.dev-dependencies]
test = [{ name = "dev-lib" }]
[[package]]
name = "directory"
version = "0.1.0"
source = { directory = "directory" }
[[package]]
name = "registry-lib"
version = "1.2.3"
source = { registry = "https://pypi.org/simple" }
wheels = [{ url = "https://example.org/lib.whl", hash = "sha256:abc", size = 123 }]
[[package]]
name = "git-lib"
version = "2.0.0"
source = { git = "https://github.com/example/lib#01234567" }
[[package]]
name = "dev-lib"
version = "3.0.0"
[[package]]
name = "transitive"
version = "4.0.0"
"#;
    assert_eq!(
        uv_lock(uv).unwrap(),
        vec![
            dep(Eco::PyPI, "registry-lib", "1.2.3", true, "uv.lock"),
            dep(Eco::PyPI, "git-lib", "2.0.0", true, "uv.lock (git)"),
            dep(Eco::PyPI, "dev-lib", "3.0.0", true, "uv.lock"),
            dep(Eco::PyPI, "transitive", "4.0.0", false, "uv.lock"),
        ]
    );
}

#[test]
fn toml_package_arrays_handle_empty_missing_and_invalid_input() {
    for parse in [cargo_lock, uv_lock] {
        for text in ["", "version = 1", "package = []", "package = 1"] {
            assert!(parse(text).unwrap().is_empty(), "{text}");
        }
        assert!(parse("[[package]\nname =").is_err());
    }
}

#[test]
fn python_and_npm_git_origins_are_not_registry_releases() {
    let uv = r#"[[package]]
name = "git-lib"
version = "1.2.3"
source = { git = "https://github.com/o/r?rev=abc#01234567" }
"#;
    assert_eq!(uv_lock(uv).unwrap()[0].from, "uv.lock (git)");
    let poetry = r#"[[package]]
name = "git-lib"
version = "1.2.3"
[package.source]
type = "git"
url = "https://github.com/o/r"
"#;
    assert_eq!(poetry_lock(poetry, "poetry.lock", &HashSet::new()).unwrap()[0].from, "poetry.lock (git)");
    let npm = r#"{"packages":{"node_modules/git-lib":{"version":"1.2.3","resolved":"git+https://github.com/o/r.git#abc"}}}"#;
    assert_eq!(npm_lock(npm, "package-lock.json").unwrap()[0].from, "package-lock.json (git)");
}

#[test]
fn npm_v3() {
    let t = r#"{"lockfileVersion":3,"packages":{"":{"dependencies":{"zod":"^3.23.0"},"devDependencies":{"@types/node":"^20"}},
          "node_modules/zod":{"version":"3.23.8"},
          "node_modules/@types/node":{"version":"20.1.0","dev":true},
          "node_modules/a/node_modules/zod":{"version":"3.0.0"},
          "node_modules/ws-pkg":{"resolved":"packages/x","link":true}}}"#;
    let d = npm_lock(t, "package-lock.json").unwrap();
    assert_eq!(d.len(), 2);
    assert!(d.iter().any(|d| d.name == "zod" && d.version == "3.23.8" && d.direct));
    assert!(d.iter().any(|d| d.name == "@types/node" && d.direct));
}

#[test]
fn pnpm_v9_and_v6() {
    let v9 = "lockfileVersion: '9.0'\nimporters:\n  .:\n    dependencies:\n      next:\n        specifier: ^15.0.0\n        version: 15.0.3(react@19.0.0)\n      '@tanstack/react-query':\n        specifier: ^5\n        version: 5.59.0(react@19.0.0)\npackages:\n  next@15.0.3:\n    resolution: {integrity: x}\n  '@swc/helpers@0.5.13':\n    resolution: {integrity: y}\n";
    let d = pnpm_lock(v9).unwrap();
    assert!(d.iter().any(|d| d.name == "next" && d.version == "15.0.3" && d.direct));
    assert!(d.iter().any(|d| d.name == "@tanstack/react-query" && d.version == "5.59.0"));
    assert!(d.iter().any(|d| d.name == "@swc/helpers" && d.version == "0.5.13" && !d.direct));
    let v5 = "lockfileVersion: 5.4\nspecifiers:\n  zod: ^3.0.0\ndependencies:\n  zod: 3.21.4\npackages:\n  /zod/3.21.4:\n    resolution: {integrity: z}\n";
    let d = pnpm_lock(v5).unwrap();
    assert_eq!(d.iter().filter(|d| d.name == "zod").count(), 1);
    assert_eq!(d[0].version, "3.21.4");
}

#[test]
fn bun_and_yarn() {
    let bun = r#"{
  "lockfileVersion": 1,
  "workspaces": { "": { "name": "app", "dependencies": { "zod": "^4.0.0", }, }, },
  "packages": {
    "zod": ["zod@4.1.5", "", {}, "sha512-x"],
    "@types/react": ["@types/react@19.0.1", "", {}, "sha512-y"],
    "a/zod": ["zod@3.0.0", "", {}, "sha512-z"],
  }
}"#;
    let d = bun_lock(bun).unwrap();
    assert_eq!(d.len(), 2);
    assert!(d.iter().any(|d| d.name == "zod" && d.version == "4.1.5" && d.direct));
    let y1 =
        "# yarn lockfile v1\n\n\"@babel/core@^7.0.0\", \"@babel/core@^7.1.0\":\n  version \"7.24.0\"\n  resolved \"x\"\n\nzod@^3.22.0:\n  version \"3.22.4\"\n";
    let direct: HashSet<String> = ["zod".to_string()].into();
    let d = yarn_lock(y1, &direct);
    assert_eq!(d.len(), 2);
    assert!(d.iter().any(|d| d.name == "@babel/core" && d.version == "7.24.0"));
    let berry = "__metadata:\n  version: 8\n\n\"zod@npm:^3.22.0\":\n  version: 3.23.8\n  resolution: \"zod@npm:3.23.8\"\n";
    let d = yarn_lock(berry, &direct);
    assert_eq!(d, vec![dep(Eco::Npm, "zod", "3.23.8", true, "yarn.lock")]);
}

#[test]
fn cargo_and_python_and_go() {
    let c = "version = 4\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\ndependencies = [\"tokio\", \"axum 0.8.1\"]\n\n[[package]]\nname = \"tokio\"\nversion = \"1.47.1\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"axum\"\nversion = \"0.8.1\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"bytes\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
    let d = cargo_lock(c).unwrap();
    assert_eq!(d.len(), 3);
    assert!(d.iter().any(|d| d.name == "axum" && d.direct));
    assert!(d.iter().any(|d| d.name == "bytes" && !d.direct));
    let g = "version = 4\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\ndependencies = [\"mygit\"]\n\n[[package]]\nname = \"mygit\"\nversion = \"0.2.0\"\nsource = \"git+https://github.com/o/r?branch=main#0123456789abcdef\"\n\n[[package]]\nname = \"local\"\nversion = \"0.1.0\"\nsource = \"path+file:///x\"\n";
    let d = cargo_lock(g).unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].from, "Cargo.lock (git)");
    assert!(d[0].direct);

    let uv = "version = 1\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\nsource = { editable = \".\" }\ndependencies = [{ name = \"pydantic\" }]\n\n[[package]]\nname = \"pydantic\"\nversion = \"2.9.2\"\nsource = { registry = \"https://pypi.org/simple\" }\n\n[[package]]\nname = \"annotated-types\"\nversion = \"0.7.0\"\nsource = { registry = \"https://pypi.org/simple\" }\n";
    let d = uv_lock(uv).unwrap();
    assert_eq!(d.len(), 2);
    assert!(d.iter().any(|d| d.name == "pydantic" && d.version == "2.9.2" && d.direct));

    let r = requirements(
        "pydantic==1.10.18  # pinned\nfastapi[all]==0.110.0 ; python_version>'3'\n-r other.txt\nrequests>=2\n",
        "requirements.txt",
    );
    assert_eq!(r.len(), 2);
    assert_eq!(r[1].name, "fastapi");

    let g = go_mod("module x\n\ngo 1.22\n\nrequire github.com/gin-gonic/gin v1.10.0\n\nrequire (\n\tgithub.com/spf13/cobra v1.8.1\n\tgolang.org/x/sys v0.20.0 // indirect\n)\n\nreplace github.com/spf13/cobra => github.com/spf13/cobra v1.8.0\n");
    assert_eq!(g.len(), 3);
    assert!(g.iter().any(|d| d.name == "github.com/spf13/cobra" && d.version == "v1.8.0" && d.direct));
    assert!(g.iter().any(|d| d.name == "golang.org/x/sys" && !d.direct));
}
