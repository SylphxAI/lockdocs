use super::args::Args;
use crate::{preview, pro};
use lockdocs_core::query::Engine;

pub(super) fn authorize<'a>(cmd: &str, args: &'a Args) -> Option<&'a str> {
    // The Pro gate runs before any download or index work. Exit code 3: Pro required.
    let upgrade_target = match cmd {
        "upgrade" => args.positional.get(1).map(String::as_str),
        "docs" | "search" => args.flag("upgrade-to"),
        _ => None,
    };
    if cmd == "upgrade" || upgrade_target.is_some() {
        match pro::require(&pro::POLICY, pro::UPGRADE_REPORT) {
            Ok(licence) => {
                if let Some(note) = pro::renewal_note(&pro::POLICY, &licence) {
                    eprintln!("{note}");
                }
            }
            Err(required) => {
                let json = args.on("json");
                let (pkg, target) = if cmd == "upgrade" {
                    (args.positional.first().map(String::as_str), upgrade_target)
                } else {
                    (args.pkg(), upgrade_target)
                };
                match (pkg, target) {
                    (Some(pkg), Some(target)) => {
                        // A local, honest preview: counts only, same download opt-ins as the report.
                        let engine = Engine::new(&args.root(), args.opts());
                        match engine.upgrade_report(pkg, target, args.tokens()) {
                            Ok(answer) => {
                                let p = preview::build(&answer.json, &required);
                                if json {
                                    println!("{}", serde_json::to_string_pretty(&p.json).unwrap_or_default());
                                } else {
                                    println!("{}", p.text);
                                }
                            }
                            Err(e) => eprintln!("{e}\n{required}"),
                        }
                    }
                    _ => eprintln!("{required}"),
                }
                std::process::exit(3);
            }
        }
    }
    upgrade_target
}
