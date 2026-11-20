//! Editor-only recovery. Recheck independent declarations with the real compiler.
//! Original lexer spans stay on every token, including those after a skipped recipe.
use super::*;
use std::ops::Range;

const MAX_RECOVERIES: usize = 16;

fn declaration(tokens: &[Token], bad: usize) -> Option<Range<usize>> {
    let mut start = 0;
    let mut braces = 0usize;
    for (i, token) in tokens.iter().enumerate() {
        let end = match token.kind {
            Kind::LeftBrace => { braces += 1; false }
            Kind::RightBrace => { braces = braces.saturating_sub(1); braces == 0 }
            Kind::Semicolon => braces == 0,
            Kind::End => {
                return (start <= bad && bad < i).then_some(start..i);
            }
            _ => false,
        };
        if end {
            let after = i + 1;
            if (start..after).contains(&bad) {
                let optional = matches!(token.kind, Kind::RightBrace)
                    && matches!(tokens.get(after).map(|t| &t.kind), Some(Kind::Semicolon));
                return Some(start..after + usize::from(optional));
            }
            start = after;
        }
    }
    None
}

fn name_at(tokens: &[Token], index: usize) -> Option<&str> {
    match &tokens.get(index)?.kind {
        Kind::Name(name) => Some(name),
        _ => None,
    }
}

pub(super) fn parse(tokens: Vec<Token>, map: &SourceMap) -> (Parser, Option<Program>, Vec<GenomeDiagnostic>) {
    let mut tokens = tokens;
    let mut errors = Vec::new();
    let mut removed_names = HashSet::new();
    for round in 0..=MAX_RECOVERIES {
        let mut parser = Parser::new(tokens);
        parser.analyzed_functions = Some(Vec::new());
        let error = match parser.program() {
            Ok(program) => return (parser, Some(program), errors),
            Err(error) => error,
        };
        let bad = parser.tokens.partition_point(|token| token.location.from < error.location.from);
        let symbol = name_at(&parser.tokens, bad);
        let cascade = error.message.starts_with("unknown ")
            && symbol.is_some_and(|word| word.split('.').any(|name| removed_names.contains(name)));
        let at_end = bad >= parser.tokens.len() - 1;
        let missing_skipped_output = at_end && !errors.is_empty()
            && error.message.starts_with("missing required output")
            && error.message.split('\'').nth(1).is_some_and(|name| removed_names.contains(name));
        let missing_separator = error.message.starts_with("expected ';'");
        let limit = error.message.contains("exceeds") || error.message.contains("limited to");
        if !cascade && !missing_skipped_output { errors.push(map.diagnostic(error)); }
        if at_end || limit || round == MAX_RECOVERIES { return (parser, None, errors); }
        let Some(mut skip) = declaration(&parser.tokens, bad) else { return (parser, None, errors); };
        if name_at(&parser.tokens, skip.start) == Some("cellular") {
            return (parser, None, errors); // Removing the header would switch the language.
        }
        if missing_separator && bad > skip.start
            && !parser.tokens[skip.start..bad].iter().any(|token| token.kind == Kind::LeftBrace)
            && symbol.is_some_and(|word|
            matches!(word, "let" | "fn" | "product" | "gene" | "assembly" | "clear" | "publish" | "signal")
            || matches!(parser.tokens.get(bad+1).map(|t| &t.kind), Some(Kind::Assign))) {
            skip.end = bad; // Preserve the declaration where the missing separator was noticed.
        }
        if let Some(head) = name_at(&parser.tokens, skip.start) {
            let name = match head {
                "let" | "fn" | "product" | "gene" | "assembly" => name_at(&parser.tokens, skip.start + 1),
                _ => Some(head),
            };
            if let Some(name) = name { removed_names.insert(name.to_string()); }
        }
        tokens = std::mem::take(&mut parser.tokens);
        tokens.drain(skip);
    }
    unreachable!()
}

/// Report controls that compile but cannot affect the requested physical operation.
pub(super) fn check_physical_controls(program: &Program, parser: &Parser, map: &SourceMap, out: &mut Vec<GenomeDiagnostic>) {
    use crate::cellular::MechanismKind::*;
    let Some(graph) = &program.cellular else { return };
    let mut warn = |location: Location, message: String| {
        let mut diagnostic = map.diagnostic(location.error(message));
        diagnostic.severity = "warning";
        out.push(diagnostic);
    };
    for (i, (recipe, geometry)) in graph.definition.assemblies.iter().zip(&parser.geometry_locations).enumerate() {
        let AssemblyKind::Mechanism(kind) = recipe.kind else { continue };
        if let Some(span) = geometry.at.filter(|_| recipe.kind.surface_footprint() == 0.0) {
            warn(span, format!("{} is internal. Its 'at' angle has no effect on placement or inheritance.", kind.name()));
        }
        if let Some(span) = geometry.direction.filter(|_| !matches!(kind, Propulsion | Pierce | SenseOptical)) {
            warn(span, format!("{} has no directional action. Its 'direction' angle has no effect.", kind.name()));
        }
        if let Some(root) = graph.activity[i].filter(|_| matches!(kind, EnergyStorage | MineralStorage | FoodStorage)) {
            let is_default = matches!(&program.nodes[root].op, Op::Constant(value) if value.number() == 1.0);
            if !is_default {
                warn(parser.node_locations[root].head, format!("{} retains its installed capacity at every activity level. Use 'start_when' to control construction or 'recycle' to remove a compartment.", kind.name()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passive_controls_are_warnings_and_keep_the_program_valid() {
        let src = "cellular counts; product Skin=membrane(); assembly Tank:store(food){ Skin:400; at:1.2; direction:0.4; activity:0; }";
        let result = Program::analyze(src);
        assert!(result.valid);
        assert!(Program::compile(src).is_ok());
        assert_eq!(result.diagnostics.len(), 3);
        assert!(result.diagnostics.iter().all(|item| item.severity == "warning"));
        assert!(result.diagnostics.iter().any(|item| &src[item.from..item.to] == "1.2"));
        assert!(Program::analyze(&src.replace("at:1.2; direction:0.4; activity:0;", "activity:1;")).diagnostics.is_empty());
    }

    #[test]
    fn independent_errors_keep_valid_later_bindings_and_utf16_spans() {
        let src = "# 🌱\ncellular; let a = bad; let ignored = a + 1; let b = true + 2; let good = 3;";
        let result = Program::analyze(src);
        assert!(!result.valid);
        assert_eq!(result.diagnostics.len(), 2);
        assert!(result.diagnostics[0].message.contains("bad"));
        assert_eq!(result.diagnostics[1].from, src[..src.rfind('+').unwrap()].encode_utf16().count());
        assert_eq!(result.bindings.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), ["good"]);
    }

    #[test]
    fn recovery_preserves_declarations_after_recipes_and_missing_semicolons() {
        let src = "cellular; product Skin = membrane(); assembly A: store(food) { Skin: 0.08; activity: false; };\nlet unfinished = 1\nlet intact = 7; let xbad = typo;";
        let result = Program::analyze(src);
        assert_eq!(result.diagnostics.len(), 3, "{:?}",result.diagnostics);
        assert_eq!(result.bindings[0].name, "intact");
        assert!(Program::compile(src).unwrap_err().ends_with(&result.diagnostics[0].message));
    }

    #[test]
    fn recovery_keeps_strings_intact_and_avoids_synthetic_missing_outputs() {
        let src = "let text = \"; } #\"; thrust=true; turn=false; divide=true;";
        let result = Program::analyze(src);
        assert_eq!(result.diagnostics.len(), 2);
        assert_eq!(result.bindings[0].name, "text");
        assert!(result.diagnostics.iter().all(|d| !d.message.contains("missing")));
    }
}
