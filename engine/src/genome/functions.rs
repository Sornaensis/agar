//! Small pure functions are type-checked at declaration and inlined at calls.
//! Scalar parameters avoid runtime call frames and keep the graph bounded.
use super::*;

const MAX_FUNCTIONS: usize = 64;
const MAX_PARAMETERS: usize = 8;

#[derive(Clone)]
pub(super) struct UserFunction {
    parameters: Vec<String>,
    tokens: Vec<Token>,
    bindings: HashMap<String, usize>,
    scope: usize,
    result_type: Type,
}

impl Parser {
    pub(super) fn function_declaration(&mut self, location: Location) -> Result<(), CompileError> {
        let token = self.take();
        let Kind::Name(name) = token.kind else {
            return Err(token.location.error("expected a function name."));
        };
        if !cellular::valid_name(&name) || name == "fn" {
            return Err(token.location.error("invalid or reserved function name."));
        }
        if self.functions.contains_key(&name) || self.bindings.contains_key(&name) {
            return Err(token
                .location
                .error(format!("'{name}' is already defined.")));
        }
        if self.functions.len() >= MAX_FUNCTIONS {
            return Err(location.error(format!(
                "programs are limited to {MAX_FUNCTIONS} functions."
            )));
        }
        self.expect(Kind::LeftParen, "'(' after function name")?;
        let mut parameters = Vec::new();
        while self.peek().kind != Kind::RightParen {
            if !parameters.is_empty() {
                self.expect(Kind::Comma, "',' between parameters")?;
            }
            let token = self.take();
            let Kind::Name(parameter) = token.kind else {
                return Err(token.location.error("expected a scalar parameter name."));
            };
            if parameter.is_empty()
                || parameter.len() > 64
                || parameter.contains('.')
                || matches!(
                    parameter.as_str(),
                    "if" | "then" | "else" | "true" | "false" | "fn"
                )
                || Register::reserved(&parameter)
            {
                return Err(token.location.error("invalid function parameter name."));
            }
            if parameters.contains(&parameter) {
                return Err(token
                    .location
                    .error(format!("duplicate parameter '{parameter}'.")));
            }
            if parameters.len() == MAX_PARAMETERS {
                return Err(token.location.error(format!(
                    "functions accept at most {MAX_PARAMETERS} scalar parameters."
                )));
            }
            parameters.push(parameter);
        }
        self.expect(Kind::RightParen, "')' after parameters")?;
        self.expect(Kind::Assign, "'=' before function expression")?;
        let body_start = self.position;
        let node_start = self.nodes.len();
        let bindings = self.bindings.clone();
        let old_scope = self.function_scope;
        self.function_scope = Some(self.binding_order.len());
        for parameter in &parameters {
            let sample = self.add(Op::Constant(Value::Scalar(0.0)), Type::Number, location)?;
            self.bindings.insert(parameter.clone(), sample);
        }
        // The new function is not registered yet: recursive and forward calls
        // are rejected even when no invocation of this function is reachable.
        let root = self.expression(0, 0)?;
        let result_type = self.nodes[root].ty;
        let body_end = self.position;
        let mut tokens = self.tokens[body_start..body_end].to_vec();
        tokens.push(Token {
            kind: Kind::End,
            location: self.peek().location,
        });
        self.expect(Kind::Semicolon, "';' after function expression")?;
        self.nodes.truncate(node_start);
        self.node_locations.truncate(node_start);
        self.bindings = bindings.clone();
        self.function_scope = old_scope;
        self.functions.insert(
            name,
            UserFunction {
                parameters,
                tokens,
                bindings,
                scope: self.binding_order.len(),
                result_type,
            },
        );
        Ok(())
    }

    pub(super) fn user_call(
        &mut self,
        name: &str,
        location: Location,
        depth: usize,
    ) -> Result<usize, CompileError> {
        if self.function_depth >= MAX_DEPTH || depth >= MAX_DEPTH {
            return Err(location.error(format!("function expansion exceeds {MAX_DEPTH} levels.")));
        }
        let function = self.functions[name].clone();
        self.expect(Kind::LeftParen, "'('")?;
        let mut arguments = Vec::with_capacity(function.parameters.len());
        for index in 0..function.parameters.len() {
            if index > 0 {
                self.expect(Kind::Comma, "',' between arguments")?;
            }
            if self.peek().kind == Kind::RightParen {
                return Err(location.error(format!(
                    "'{name}' expects {} argument(s).",
                    function.parameters.len()
                )));
            }
            let argument_start = self.peek().location;
            let mut argument = self.expression(0, depth)?;
            self.assignable(argument, Type::Number, argument_start)?;
            if matches!(self.nodes[argument].ty,Type::Integer | Type::Count) {
                argument = self.add(
                    Op::Call(Function::Float, [argument, 0, 0]),
                    Type::Number,
                    argument_start,
                )?;
                self.node_locations[argument].authored = false;
            }
            arguments.push(argument);
        }
        if self.peek().kind == Kind::Comma {
            return Err(location.error(format!(
                "'{name}' expects {} argument(s).",
                function.parameters.len()
            )));
        }
        self.expect(Kind::RightParen, "')' after arguments")?;
        let call_end = self.tokens[self.position - 1].location.to;
        let resume_position = self.position;
        let old_tokens = std::mem::replace(&mut self.tokens, function.tokens);
        let old_bindings = std::mem::replace(&mut self.bindings, function.bindings);
        let old_scope = self.function_scope.replace(function.scope);
        for (parameter, argument) in function.parameters.into_iter().zip(arguments) {
            self.bindings.insert(parameter, argument);
        }
        self.position = 0;
        self.function_depth += 1;
        let node_start = self.nodes.len();
        // Carry the caller's expression depth into the expansion. Resetting it
        // here would allow nested parentheses in a chain of functions to
        // multiply the parser's stack depth despite each body passing alone.
        let parsed = self.expression(0, depth);
        self.function_depth -= 1;
        self.tokens = old_tokens;
        self.position = resume_position;
        self.bindings = old_bindings;
        self.function_scope = old_scope;
        let root = parsed?;
        debug_assert_eq!(self.nodes[root].ty, function.result_type);
        // Each invocation gets one source span. Inlined internals stay in the
        // physical graph, but aren't misreported as shared definition values or
        // mutated as though one source leaf represented only one invocation.
        for node in &mut self.node_locations[node_start..] {
            node.from = location.from;
            node.to = call_end;
            node.head = location;
            node.authored = false;
        }
        let result = self.add(Op::Reference(root), function.result_type, location)?;
        self.node_locations[result].to = call_end;
        Ok(result)
    }
}
