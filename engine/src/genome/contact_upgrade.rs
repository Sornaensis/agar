//! Translate the two retired contact directives at import boundaries. Edits use
//! declaration and recipe spans from the historical parser, in byte offsets.
use super::*;
use crate::body::OrganelleKind;

impl Program {
    pub(crate) fn upgrade_contact_source(source: &str) -> Result<Option<String>, String> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(format!("Program exceeds {MAX_SOURCE_BYTES} bytes."));
        }
        if !source.contains("avoid_message") && !source.contains("adhere_message") {
            return Ok(None);
        }
        // Current documents may mention old syntax in comments or strings.
        if Self::compile_stored(source).is_ok() {
            return Ok(None);
        }
        let mut parser = Parser::new(lex(source).map_err(|e| e.to_string())?);
        let historical = parser
            .program_for_contact_upgrade()
            .map_err(|e| e.to_string())?;
        let Some(graph) = &historical.cellular else {
            return Ok(None);
        };
        let Some(messages) = &graph.messages else {
            return Ok(None);
        };

        let mut edits = Vec::new();
        for span in parser.message_locations[1..=2].iter().flatten() {
            let mut comments = String::new();
            for line in source[span.from..span.to].lines() {
                if let Some(start) = line.find('#') {
                    comments.push_str(&line[start..]);
                    comments.push('\n');
                }
            }
            edits.push((span.from, span.to, comments));
        }
        if edits.is_empty() {
            return Ok(None);
        }
        let weapon = messages.addresses[1].map(|index| {
            format!(
                "!populated(rf{index:03}) || !peer.message_valid || peer.message != rf{index:03}"
            )
        });
        let adhesive = messages.addresses[2].map(|selector| {
            let Some(published) = messages.addresses[0] else {
                return "false".to_string();
            };
            let mut terms = Vec::new();
            if !messages.recruits_contacts {
                terms.push("peer.division_interface".to_owned());
            }
            if published != selector {
                terms.push(format!("populated(rf{published:03})"));
            }
            terms.push(format!("populated(rf{selector:03})"));
            terms.push(format!(
                "peer.message_valid && peer.message == rf{selector:03}"
            ));
            terms.join(" && ")
        });
        for (assembly, geometry) in graph
            .definition
            .assemblies
            .iter()
            .zip(&parser.geometry_locations)
        {
            use crate::cellular::MechanismKind::{Adhere, Engulf, Pierce};
            let guard = match assembly.kind {
                AssemblyKind::Mechanism(Pierce | Engulf)
                | AssemblyKind::Organelle(OrganelleKind::Piercer | OrganelleKind::Phagocyte) => {
                    weapon.as_ref()
                }
                AssemblyKind::Mechanism(Adhere) => adhesive.as_ref(),
                _ => None,
            };
            if let Some(guard) = guard {
                let close = geometry.close.from;
                let line = source[..close].rsplit('\n').next().unwrap_or("");
                let field = if line.chars().all(char::is_whitespace) {
                    format!("  engage_when: {guard};\n{line}")
                } else {
                    format!(" engage_when: {guard}; ")
                };
                edits.push((close, close, field));
            }
        }
        let mut next = source.to_string();
        edits.sort_unstable_by_key(|edit| edit.0);
        for (from, to, text) in edits.into_iter().rev() {
            next.replace_range(from..to, &text);
        }
        Ok(Some(next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIOR: &str = r#"cellular;
# An old note mentions avoid_message. 🌱
product A=anchor(); product F=filament(); product E=enzyme();
product W=membrane(); product M=motor(); product S=scaffold(); product G=adhesin(S);
assembly Fang:pierce(){A:0.01; F:0.03; E:0.01;}
assembly Mouth:engulf(){A:0.01; W:0.03; M:0.01;}
assembly Dock:adhere(){A:0.01; W:0.01; G:0.01;}
publish rf001=0; signal rf005=3;
adhere_message # keep the note inside this retired declaration
rf003 daughters; avoid_message rf002;
rf002=0; rf003=0;
"#;

    #[test]
    fn old_rules_expand_locally_without_changing_text_inside_comments_or_channel_initializers() {
        assert!(Program::compile(PRIOR).is_err());
        let next = Program::upgrade_contact_source(PRIOR).unwrap().unwrap();
        assert!(next.contains("# An old note mentions avoid_message. 🌱"));
        assert!(next.contains("# keep the note inside this retired declaration"));
        assert!(next.contains("publish rf001=0; signal rf005=3;"));
        assert_eq!(next.matches("engage_when:").count(), 3);
        let p = Program::compile(&next).unwrap();
        let mut memory = Registers::default();
        p.initialize_message(&mut memory);
        let mut frozen = Vec::new();
        let mut scratch = Vec::new();
        let peer = PeerContact {
            message: Some(0.0),
            signal: None,
            division_interface: true,
        };
        p.freeze_contact_registers(&memory, &mut frozen);
        assert!(p.allows_contact(0, &frozen, peer, &mut scratch));
        assert!(p.allows_contact(1, &frozen, peer, &mut scratch));
        assert!(!p.allows_contact(2, &frozen, peer, &mut scratch));
        for index in [2, 3] {
            memory.entries.insert(
                Register {
                    index,
                    ty: Type::Number,
                },
                Value::Scalar(0.0),
            );
        }
        p.freeze_contact_registers(&memory, &mut frozen);
        assert!(!p.allows_contact(0, &frozen, peer, &mut scratch));
        assert!(!p.allows_contact(1, &frozen, peer, &mut scratch));
        assert!(p.allows_contact(2, &frozen, peer, &mut scratch));
        assert!(!p.allows_contact(
            2,
            &frozen,
            PeerContact {
                division_interface: false,
                ..peer
            },
            &mut scratch
        ));
        let recruit =
            Program::upgrade_contact_source(&PRIOR.replace("rf003 daughters", "rf003 recruit"))
                .unwrap()
                .unwrap();
        let r = Program::compile(&recruit).unwrap();
        r.freeze_contact_registers(&memory, &mut frozen);
        assert!(r.allows_contact(
            2,
            &frozen,
            PeerContact {
                division_interface: false,
                ..peer
            },
            &mut scratch
        ));
        assert_eq!(Program::upgrade_contact_source(&next).unwrap(), None);
    }
}
