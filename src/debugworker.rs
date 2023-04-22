use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use pest_meta::{optimizer::OptimizedRule, parse_and_optimize, parser::rename_meta_rule};
use pest_vm::Vm;
use serde::{Deserialize, Serialize};

use yew_agent::{HandlerId, Public, WorkerLink};
/// Events that are sent from the debugger.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DebuggerEvent {
    /// A breakpoint encountered.
    /// The first element is the rule name.
    /// The second element is the position.
    Breakpoint(String, usize),
    /// The end of the input has been reached.
    Eof,
    /// A parsing error encountered.
    Error(String),
    /// Grammar rule names
    Rules(Vec<String>),
}

/// Debugger for pest grammars.
#[derive(Default)]
pub struct DebuggerContext {
    grammar: Option<Vec<OptimizedRule>>,
    input: Option<String>,
    breakpoints: HashSet<String>,
}

impl DebuggerContext {
    /// Loads a grammar from a string.
    pub fn load_grammar_direct(&mut self, grammar: &str) -> Result<(), String> {
        self.grammar = Some(DebuggerContext::parse_grammar(grammar)?);

        Ok(())
    }

    /// Loads a parsing input from a string.
    pub fn load_input_direct(&mut self, input: String) {
        self.input = Some(input);
    }

    /// Adds all grammar rules as breakpoints.
    /// This is useful for stepping through the entire parsing process.
    /// It returns an error if the grammar hasn't been loaded yet.
    pub fn add_all_rules_breakpoints(&mut self) -> Result<(), String> {
        let ast = self
            .grammar
            .as_ref()
            .ok_or("DebuggerError::GrammarNotOpened".to_string())?;
        for rule in ast {
            self.breakpoints.insert(rule.name.clone());
        }

        Ok(())
    }

    /// Adds a rule to breakpoints.
    pub fn add_breakpoint(&mut self, rule: String) {
        self.breakpoints.insert(rule);
    }

    /// Removes a rule from breakpoints.
    pub fn delete_breakpoint(&mut self, rule: &str) {
        self.breakpoints.remove(rule);
    }

    /// Removes all breakpoints.
    pub fn delete_all_breakpoints(&mut self) {
        self.breakpoints.clear();
    }

    fn handle(
        &self,
        ast: Vec<OptimizedRule>,
        rule: String,
        input: String,
        rsender: WorkerLink<Worker>,
        handler_id: HandlerId,
    ) {
        let breakpoints = self.breakpoints.clone();
        // FIXME: this is currently unnecessary, unless
        // there's a way to spawn a thread in WASM
        // that can be paused/resumed.
        let events = Arc::new(Mutex::new(vec![]));
        let events2 = events.clone();
        let vm = Vm::new_with_listener(
            ast,
            Box::new(move |rule, pos| {
                if breakpoints.contains(&rule) {
                    // FIXME: limit the size of events?
                    events2
                        .lock()
                        .unwrap()
                        .push(DebuggerEvent::Breakpoint(rule, pos.pos()));
                }
                false
            }),
        );
        let rrsender = rsender.clone();
        let send_events = move || {
            let events = events.lock().unwrap();
            for event in events.iter() {
                rrsender.respond(handler_id, event.clone());
            }
        };
        match vm.parse(&rule, &input) {
            Ok(_) => {
                send_events();
                rsender.respond(handler_id, DebuggerEvent::Eof)
            }
            Err(error) => {
                send_events();
                rsender.respond(handler_id, DebuggerEvent::Error(error.to_string()))
            }
        };
    }

    fn parse_grammar(grammar: &str) -> Result<Vec<OptimizedRule>, String> {
        match parse_and_optimize(grammar) {
            Ok((_, ast)) => Ok(ast),
            Err(errors) => {
                let msg = format!(
                    "error parsing\n\n{}",
                    errors
                        .iter()
                        .cloned()
                        .map(|error| format!("{}", error.renamed_rules(rename_meta_rule)))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                Err(msg)
            }
        }
    }

    /// Starts a debugger session: runs a rule on an input and stops at breakpoints.
    /// When the debugger is stopped, an event is sent to the channel using `sender`.
    /// The debugger can be resumed by calling `cont`.
    /// This naturally returns errors if the grammar or input haven't been loaded yet etc.
    pub fn run(
        &mut self,
        rule: &str,
        rsender: WorkerLink<Worker>,
        handler_id: HandlerId,
    ) -> Result<(), String> {
        let ast = self
            .grammar
            .as_ref()
            .ok_or("DebuggerError::GrammarNotOpened".to_owned())?;
        match self.input {
            Some(ref input) => {
                let rule = rule.to_owned();
                let input = input.clone();

                self.handle(ast.clone(), rule, input, rsender, handler_id);
                Ok(())
            }
            None => Err("DebuggerError::InputNotOpened".to_owned()),
        }
    }
}

/// The worker that runs the parsing process / debugger.
/// Given it doesn't pause the parsing process when hitting a breakpoint,
/// it doesn't seem necessary to run it in a worker.
/// Anyway, it's kept in case there's a way to mimic that parsing pausing/resuming
/// behaviour in WASM.
pub struct Worker {
    link: WorkerLink<Self>,
    debugger_context: DebuggerContext,
}

/// Possible messages that can be sent to the worker.
#[derive(Serialize, Deserialize)]
pub enum WorkerInput {
    /// Loads a grammar from a string.
    LoadGrammar(String),
    /// Loads a parsing input from a string.
    LoadInput(String),
    /// Adds a breakpoint at a provided rule name.
    AddBreakpoint(String),
    /// Removes a breakpoint at a provided rule name.
    DeleteBreakpoint(String),
    /// Removes all breakpoints.
    DeleteAllBreakpoints,
    /// Adds all grammar rules as breakpoints.
    AddAllRulesBreakpoints,
    /// Starts a debugger session on a provided rule.
    Run(String),
}

impl yew_agent::Worker for Worker {
    type Input = WorkerInput;
    type Message = ();
    type Output = DebuggerEvent;
    type Reach = Public<Self>;
    fn create(link: WorkerLink<Self>) -> Self {
        Self {
            link,
            debugger_context: Default::default(),
        }
    }

    fn update(&mut self, _msg: Self::Message) {
        // no messaging
    }

    fn handle_input(&mut self, msg: Self::Input, id: HandlerId) {
        // this runs in a web worker
        // and does not block the main
        // browser thread!
        match msg {
            WorkerInput::LoadGrammar(ref grammar) => {
                match self.debugger_context.load_grammar_direct(grammar) {
                    Ok(_) => {
                        let rules = self
                            .debugger_context
                            .grammar
                            .as_ref()
                            .unwrap()
                            .iter()
                            .map(|x| x.name.clone())
                            .collect();
                        self.link.respond(id, DebuggerEvent::Rules(rules));
                    }
                    Err(error) => {
                        self.link.respond(id, DebuggerEvent::Error(error));
                    }
                }
            }
            WorkerInput::LoadInput(input) => {
                self.debugger_context.load_input_direct(input);
            }
            WorkerInput::Run(ref rule) => {
                match self.debugger_context.run(rule, self.link.clone(), id) {
                    Ok(_) => {}
                    Err(error) => {
                        self.link.respond(id, DebuggerEvent::Error(error));
                    }
                }
            }
            WorkerInput::AddBreakpoint(rule) => {
                self.debugger_context.add_breakpoint(rule);
            }
            WorkerInput::DeleteBreakpoint(rule) => {
                self.debugger_context.delete_breakpoint(&rule);
            }
            WorkerInput::DeleteAllBreakpoints => {
                self.debugger_context.delete_all_breakpoints();
            }
            WorkerInput::AddAllRulesBreakpoints => {
                let _ = self.debugger_context.add_all_rules_breakpoints();
            }
        }
    }

    fn name_of_resource() -> &'static str {
        "worker.js"
    }

    fn resource_path_is_relative() -> bool {
        true
    }
}
