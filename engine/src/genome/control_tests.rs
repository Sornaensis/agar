use super::*;
use crate::checkpoint::{Reader, Writer};

const RECEPTORS: &str = r#"cellular;
product A=anchor(); product R=receptor(); product P=pigment(); product F=filament();
product Signal=regulator(8);
assembly Left: sense(light) { A:0.005; R:0.01; P:0.005; at:1.57; }
assembly Heat: sense(heat) { A:0.005; R:0.015; }
assembly Nose: sense(chemical) { A:0.005; R:0.015; }
assembly Touch: sense(contact) { A:0.005; R:0.01; F:0.005; }
assembly Eye: sense(optical) { A:0.005; R:0.01; P:0.005; }
"#;

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-12, "{a} != {b}");
}

#[test]
fn concentration_dilutes_with_area_and_response_helpers_have_bounded_endpoints() {
    let source = "cellular; product Signal=regulator(8); gene a:Signal=activate(concentration.Signal,0.1); gene b:Signal=repress(concentration.Signal,0.1);";
    let program = Program::compile(source).unwrap();
    for (mass, area, activation) in [(0., 1., 0.), (0.1, 1., 0.5), (0.1, 2., 1. / 3.)] {
        let input = Inputs {
            products: vec![mass],
            body_area: area,
            ..Inputs::default()
        };
        let mut values = Vec::new();
        program.evaluate_nodes(&input, &Registers::default(), &mut values);
        let graph = program.cellular.as_ref().unwrap();
        close(values[graph.genes[0].root].number(), activation);
        close(values[graph.genes[1].root].number(), 1. - activation);
        close(program.evaluate(&input).cellular.unwrap().expression[0], 1.);
    }
    for (x, k, a, r) in [(0., 0., 0., 1.), (-1., 1., 0., 1.), (1., -1., 1., 0.)] {
        let source = format!(
            "cellular;product S=regulator(1);gene a:S=activate({x},{k});gene r:S=repress({x},{k});"
        );
        let p = Program::compile(&source).unwrap();
        let mut values = Vec::new();
        p.evaluate_nodes(&Inputs::default(), &Registers::default(), &mut values);
        let graph = p.cellular.as_ref().unwrap();
        close(values[graph.genes[0].root].number(), a);
        close(values[graph.genes[1].root].number(), r);
    }
    for area in [0., -1., f64::INFINITY] {
        assert_eq!(
            evaluate_operation(
                &Op::Concentration(0),
                Type::Number,
                &Inputs {
                    body_area: area,
                    products: vec![1.],
                    ..Inputs::default()
                },
                &Registers::default(),
                &[]
            ),
            Value::Scalar(0.)
        );
    }
}

#[test]
fn named_receptor_reads_are_typed_gated_and_only_reachable_channels_are_requested() {
    let source=format!("{RECEPTORS} let unused=sensed.Eye.area; gene a:Signal=if sensed.Left.valid then sensed.Left.light else 0; gene b:Signal=sensed.Touch.speed;");
    let program = Program::compile(&source).unwrap();
    assert_eq!(
        program.receptor_reads(),
        &[
            (0, ReceptorField::Valid),
            (0, ReceptorField::Level),
            (3, ReceptorField::Speed)
        ]
    );
    let mut inputs = Inputs {
        receptors: vec![ReceptorInput::default(); 5],
        ..Inputs::default()
    };
    inputs.receptors[0] = ReceptorInput {
        available: true,
        valid: true,
        level: 0.25,
        ..ReceptorInput::default()
    };
    inputs.receptors[3] = ReceptorInput {
        available: true,
        valid: false,
        speed: 0.5,
        pressure: 1.,
        ..ReceptorInput::default()
    };
    close(
        program.evaluate(&inputs).cellular.unwrap().expression[4],
        0.75,
    );
    assert_eq!(
        ReceptorField::Pressure.read(inputs.receptors.get(3)),
        Value::Scalar(0.)
    );
    inputs.receptors[0].valid = false;
    close(
        program.evaluate(&inputs).cellular.unwrap().expression[4],
        0.5,
    );
    inputs.receptors[3].available = false;
    close(
        program.evaluate(&inputs).cellular.unwrap().expression[4],
        0.,
    );
    for read in [
        "sensed.Left.offset",
        "sensed.Nose.level",
        "sensed.Eye.token",
        "sensed.Missing.level",
    ] {
        assert!(
            Program::compile(&format!("{RECEPTORS}gene wrong:Signal={read};")).is_err(),
            "accepted {read}"
        );
    }
    assert!(Program::compile(
        "cellular; product X=scaffold();gene g:X=sensed.S.level;assembly S:support(){X:0.04;}"
    )
    .is_err());
    let forward="cellular;product A=anchor();product R=receptor();product P=pigment();gene g:P=sensed.E.level;assembly E:sense(light){A:0.005;R:0.01;P:0.005;}";
    assert!(Program::compile(forward).is_ok());
}

#[test]
fn initiation_recycling_and_activity_use_one_snapshot_and_reuse_control_buffers() {
    let source="cellular; product M=membrane(); gene walls:M=1; assembly Tank:store(food){M:0.04;start_when:pool.M<0.1;recycle:energy/10;activity:0.4;}";
    let p = Program::compile(source).unwrap();
    let mut output = Outputs::default();
    let mut memory = Registers::default();
    let mut scratch = EvalScratch::default();
    let input = Inputs {
        energy: 5.,
        products: vec![0.02],
        ..Inputs::default()
    };
    p.evaluate_cell_into(&input, &mut memory, &mut scratch, &mut output);
    let controls = output.cellular.as_ref().unwrap();
    assert_eq!(controls.start, [true]);
    assert_eq!(controls.recycle, [0.5]);
    assert_eq!(controls.activity, [0.4]);
    let pointers = (
        controls.expression.as_ptr(),
        controls.activity.as_ptr(),
        controls.start.as_ptr(),
        controls.recycle.as_ptr(),
    );
    for _ in 0..5 {
        p.evaluate_cell_into(&input, &mut memory, &mut scratch, &mut output);
    }
    let controls = output.cellular.as_ref().unwrap();
    assert_eq!(
        pointers,
        (
            controls.expression.as_ptr(),
            controls.activity.as_ptr(),
            controls.start.as_ptr(),
            controls.recycle.as_ptr()
        )
    );
    let empty = Program::compile("cellular;").unwrap();
    empty.evaluate_cell_into(&input, &mut memory, &mut scratch, &mut output);
    let c = output.cellular.as_ref().unwrap();
    assert!(c.expression.is_empty() && c.start.is_empty() && c.recycle.is_empty());
    p.evaluate_cell_into(
        &Inputs {
            energy: 20.,
            products: vec![0.2],
            ..Inputs::default()
        },
        &mut memory,
        &mut scratch,
        &mut output,
    );
    let c = output.cellular.unwrap();
    assert_eq!(c.start, [false]);
    assert_eq!(c.recycle, [1.]);
    let defaults =
        Program::compile("cellular;product M=membrane();assembly T:store(food){M:0.04;}")
            .unwrap()
            .evaluate(&input)
            .cellular
            .unwrap();
    assert_eq!(defaults.start, [true]);
    assert_eq!(defaults.recycle, [0.]);
    for field in ["start_when:1;", "recycle:true;", "activity:false;"] {
        assert!(Program::compile(&format!(
            "cellular;product M=membrane();assembly T:store(food){{M:0.04;{field}}}"
        ))
        .is_err());
    }
}

#[test]
fn pure_functions_inline_typed_numeric_calls_and_capture_only_prior_bindings() {
    let source = r#"cellular;
product Signal=regulator(8);
let factor=0.1;
fn scale(x)=x*factor;
fn feedback(x,k)=scale(repress(x,k));
fn positive(x)=x>0;
fn doubled(x)=vec(x,x);
gene a:Signal=feedback(1,1);
gene b:Signal=if positive(2) then x(doubled(3))*0.1 else 0;
"#;
    let p = Program::compile(source).unwrap();
    close(
        p.evaluate(&Inputs::default()).cellular.unwrap().expression[0],
        0.35,
    );
    let mut values = Vec::new();
    p.evaluate_nodes(&Inputs::default(), &Registers::default(), &mut values);
    let graph = p.cellular.as_ref().unwrap();
    assert_eq!(
        p.evaluate(&Inputs::default()).cellular.unwrap(),
        graph.controls(&graph.roots(), &values)
    );
    let json = Program::evaluation_json(source, &Inputs::default(), &Registers::default(), 1, 1, 0)
        .unwrap();
    assert!(json.contains("\"mode\":\"cellular\""));
    assert!(!p
        .nodes
        .iter()
        .any(|node| matches!(node.op, Op::Register(_) | Op::Populated(_))));
}

#[test]
fn pure_functions_reject_recursion_bad_unused_bodies_wrong_types_and_unbounded_expansion() {
    for body in [
        "fn f(x)=f(x);",
        "fn f(x)=g(x);fn g(x)=x;",
        "fn f(x)=x+false;",
        "fn f(x,x)=x;",
        "fn f(x)=later;let later=1;",
        "fn f(x)=x;gene a:S=f(true);",
        "fn f(x)=x;gene a:S=f();",
        "fn f(x)=x;gene a:S=f(1,2);",
        "fn f(a,b,c,d,e,f,g,h,i)=a;",
        "fn f(x)=x;let f=1;",
    ] {
        assert!(
            Program::compile(&format!("cellular;product S=regulator(1);{body}")).is_err(),
            "accepted {body}"
        );
    }
    let mut source = "cellular;product S=regulator(1);fn f0(x)=x;".to_string();
    for i in 1..18 {
        source.push_str(&format!("fn f{i}(x)=f{}(x)+f{}(x);", i - 1, i - 1));
    }
    source.push_str("gene a:S=f17(1);");
    assert!(Program::compile(&source)
        .unwrap_err()
        .contains("expression nodes"));
    let mut nested = "cellular;fn base(x)=x;".to_string();
    for i in 0..10 {
        let callee = if i == 0 {
            "base".to_string()
        } else {
            format!("f{}", i - 1)
        };
        nested.push_str(&format!(
            "fn f{i}(x)={}{}(x){};",
            "(".repeat(10),
            callee,
            ")".repeat(10)
        ));
    }
    assert!(Program::compile(&nested).unwrap_err().contains("nesting"));
}

#[test]
fn control_graph_checkpoints_round_trip_and_old_v6_gets_default_controls() {
    let source=format!("{RECEPTORS}fn demand(x)=repress(x,0.1);gene a:Signal=demand(concentration.Signal)+sensed.Left.level;");
    let p = Program::compile(&source).unwrap();
    let mut w = Writer::new();
    p.write_checkpoint(&mut w);
    let mut reader = Reader::new(&w.0);
    let restored = Program::read_checkpoint(&mut reader).unwrap();
    assert_eq!(reader.remaining(), 0);
    assert_eq!(restored.graph_json(), p.graph_json());
    assert!(restored.checkpoint_source_matches(&p, &source, true));
    assert!(Program::read_checkpoint_version(&mut Reader::new(&w.0), 6).is_err());
    let old = "cellular;product M=membrane();gene g:M=0.5;assembly T:store(food){M:0.04;}";
    let p = Program::compile(old).unwrap();
    let mut w = Writer::new();
    p.write_checkpoint(&mut w);
    // v7's only addition to this otherwise-v6 graph is two absent root tags.
    w.0.truncate(w.0.len() - 8);
    let mut reader = Reader::new(&w.0);
    let restored = Program::read_checkpoint_version(&mut reader, 6).unwrap();
    assert_eq!(reader.remaining(), 0);
    assert!(restored.checkpoint_source_matches(&p, old, true));
    assert_eq!(
        restored.evaluate(&Inputs::default()),
        p.evaluate(&Inputs::default())
    );
    let mut invalid = Program::compile(&source).unwrap();
    let receptor = invalid
        .nodes
        .iter_mut()
        .find(|n| matches!(n.op, Op::Receptor(_, ReceptorField::Level)))
        .unwrap();
    receptor.ty = Type::Bool;
    let mut w = Writer::new();
    invalid.write_checkpoint(&mut w);
    assert!(Program::read_checkpoint(&mut Reader::new(&w.0)).is_err());
}

#[test]
fn geometry_mutations_change_one_future_angle_and_preserve_material_definitions() {
    let source = format!("# inherited recipes remain physical\n{RECEPTORS}");
    let p = Program::compile(&source).unwrap();
    for seed in 1..30 {
        let changed = Program::mutate_source(&source, seed, MutationKind::Geometry).unwrap();
        let again = Program::mutate_source(&source, seed, MutationKind::Geometry).unwrap();
        assert_eq!(changed.source, again.source);
        assert_eq!(changed.kind, MutationKind::Geometry);
        assert!(changed
            .source
            .starts_with("# inherited recipes remain physical\n"));
        let before = p.cellular().unwrap();
        let after = changed.program.cellular().unwrap();
        assert_eq!(before.products, after.products);
        let mut differences = 0;
        for (a, b) in before.assemblies.iter().zip(&after.assemblies) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.parts, b.parts);
            differences += usize::from(a.at != b.at) + usize::from(a.direction != b.direction);
            assert!(
                b.at.abs() <= std::f64::consts::PI && b.direction.abs() <= std::f64::consts::PI
            );
        }
        assert_eq!(differences, 1);
        let mut w = Writer::new();
        changed.program.write_checkpoint(&mut w);
        assert!(Program::read_checkpoint(&mut Reader::new(&w.0))
            .unwrap()
            .checkpoint_source_matches(&changed.program, &changed.source, true));
    }
    assert!((1..200)
        .filter_map(|seed| Program::mutate_source(&source, seed, MutationKind::Any))
        .any(|m| m.kind == MutationKind::Geometry));
}

#[test]
fn authoring_requires_named_receptors_and_marker_values_can_mutate() {
    for global in ["contact.valid", "chemical.organic_bearing", "light.level", "heat.value", "object.area", "flow.speed"] {
        let source = format!("cellular counts; let measure={global};");
        assert!(Program::compile(&source).unwrap_err().contains("named receptor"));
        assert!(Program::compile_stored(&source).is_ok());
    }
    for source in ["cellular counts; publish rf001; rf001=17;", "cellular counts; publish rf001=17;", "cellular; publish rf001=17;"] {
        let mutated = Program::mutate_source(source, 19, MutationKind::Constant).unwrap();
        assert_ne!(source, mutated.source);
        assert!(Program::compile(&mutated.source).is_ok());
    }
}
