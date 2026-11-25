use super::Spec;
use crate::{Dep, Eco};

#[test]
fn specs() {
    assert_eq!(
        Spec::parse("zod"),
        Spec {
            eco: None,
            name: "zod".into(),
            version: None
        }
    );
    assert_eq!(
        Spec::parse("npm:@tanstack/react-query@5.1.0"),
        Spec {
            eco: Some(Eco::Npm),
            name: "@tanstack/react-query".into(),
            version: Some("5.1.0".into())
        }
    );
    assert_eq!(Spec::parse("pydantic==2.9.2").version.as_deref(), Some("2.9.2"));
    assert_eq!(Spec::parse("github.com/gin-gonic/gin@v1.10.0").version.as_deref(), Some("1.10.0"));
    let s = Spec::parse("zod@3");
    assert!(s.version_matches("3.23.8") && !s.version_matches("4.0.1") && !s.version_matches("30.0.0"));
    let d = Dep {
        eco: Eco::Go,
        name: "github.com/go-chi/chi/v5".into(),
        version: "v5.1.0".into(),
        direct: true,
        from: "go.mod".into(),
    };
    assert!(Spec::parse("chi").name_matches(&d));
    let d = Dep {
        eco: Eco::PyPI,
        name: "Typing_Extensions".into(),
        version: "4".into(),
        direct: true,
        from: "x".into(),
    };
    assert!(Spec::parse("typing-extensions").name_matches(&d));
}
