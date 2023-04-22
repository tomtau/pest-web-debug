mod debugworker;
pub use debugworker::Worker;
use debugworker::{DebuggerEvent, WorkerInput};

use std::{collections::VecDeque, rc::Rc};

use wasm_bindgen::JsCast;

use web_sys::{HtmlDialogElement, HtmlInputElement, HtmlSelectElement, HtmlTextAreaElement};
use yew::prelude::*;
use yew_agent::{Bridge, Bridged};

/// The state of the web debugger.
/// FIXME: derive Properties and use it to avoid
/// copying the state content.
pub struct AppState {
    /// the (unparsed) grammar text from the textarea
    pub grammar: String,
    /// the input text from the textarea
    pub input: String,
    /// the list of breakpoints
    /// the form is: (enabled, rule_name)
    pub breakpoints: Vec<(bool, String)>,
    /// the list of events to display / go through
    /// (encountered breakpoints)
    pub events: VecDeque<DebuggerEvent>,
    /// the rule selected to be run
    pub to_run: String,
    /// whether the debugger session is currently in progress
    pub running: bool,
    /// the error message, if any
    pub error: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            grammar: r#"alpha = { 'a'..'z' | 'A'..'Z' }

digit = { '0'..'9' }

ident = { (alpha | digit)+ }

ident_list = _{ !digit ~ ident ~ (" " ~ ident)+ }"#
                .to_owned(),
            input: String::from("hello world"),
            breakpoints: vec![
                (false, "alpha".to_owned()),
                (false, "digit".to_owned()),
                (false, "ident".to_owned()),
                (false, "ident_list".to_owned()),
            ],
            events: VecDeque::new(),
            to_run: "ident_list".to_owned(),
            running: false,
            error: None,
        }
    }
}

/// The main web component.
pub struct App {
    /// the grammar textarea
    grammar_ref: NodeRef,
    /// the input textarea
    input_ref: NodeRef,
    /// the error modal dialog
    modal_ref: NodeRef,
    /// for the communication with the debugger worker
    worker: Box<dyn Bridge<Worker>>,
    /// the state of the web debugger
    state: AppState,
}

/// The possible UI messages.
pub enum Message {
    /// the grammar textarea was modified
    GrammarChange,
    /// the input textarea was modified
    InputChange,
    /// the "Run" button was clicked
    Run,
    /// the "Continue" button was clicked
    Continue,
    /// the "Stop" button was clicked
    Stop,
    /// the "Add all breakpoint" button was clicked
    AddAllBreakpoints,
    /// the "Remove all breakpoint" button was clicked
    RemoveAllBreakpoints,
    /// the selection of the rule to run was changed
    SelectRuleToRun(Event),
    /// the breakpoint was ticked or unticked
    ChangeBreakpoint(Event),
    /// the worker sent a message
    WorkerMsg(DebuggerEvent),
}

impl App {
    fn input_display(&self, ctx: &Context<Self>) -> Html {
        if !self.state.running {
            html! {
                <div class="half">
                    <label for="parser-input">{"Input to parse"}</label>
                    <textarea id="parser-input"  name="parser-input" class="parser-input nes-textarea" rows="20" cols="33"
                    ref={self.input_ref.clone()} value={self.state.input.clone()} oninput={ctx.link().callback(|_| Message::InputChange)}> </textarea>
                </div>
            }
        } else {
            let span = self.state.events.front();
            if let Some(DebuggerEvent::Breakpoint(_, start_idx)) = span {
                // TODO: will this display fail with non-ASCII characters?
                let input = self.state.input.chars();
                let start = input.clone().take(*start_idx).collect::<String>();
                let rest = input.skip(*start_idx);
                let rest_1 = rest
                    .clone()
                    .take(1)
                    .collect::<String>()
                    .replace(' ', "␣")
                    .replace('\r', "␍\r")
                    .replace('\n', "␊\n");
                let rest_1 = if rest_1.is_empty() {
                    String::from("␃")
                } else {
                    rest_1
                };
                let rest_2 = rest.skip(1).collect::<String>();
                html! {
                    <div class="half">
                        <label for="parser-input">{"Input to parse"}</label>
                        <div id="parser-input"  name="parser-input" class="parser-input nes-textarea">
                            {start} <span class="nes-text is-primary is-dark">{rest_1}</span> {rest_2}
                        </div>
                    </div>
                }
            } else {
                html! {
                    <div class="half">
                        <label for="parser-input">{"Input to parse"}</label>
                        <div id="parser-input"  name="parser-input" class="parser-input nes-textarea">
                            {self.state.input.clone()}
                        </div>
                    </div>
                }
            }
        }
    }

    fn control_height(&self) -> usize {
        320 + (self.state.breakpoints.len().saturating_sub(3) * 50)
    }

    fn controls(&self, ctx: &Context<Self>) -> Html {
        let style = format!(
            "clear:both; margin:20px;width: 62%; height:{}px",
            self.control_height()
        );
        let enabled_button = "nes-btn".to_owned();
        let disabled_button = "nes-btn is-disabled".to_owned();
        let buttons = if self.state.running {
            html! {
                <>
                    <button type="button" class={disabled_button.clone()}>{"Run"}</button>
                    <button type="button" class={enabled_button.clone() + " is-primary"} onclick={ctx.link().callback(|_| Message::Continue)}>{"Continue"}</button>
                    <button type="button" class={enabled_button.clone() + " is-warning"} onclick={ctx.link().callback(|_| Message::Stop)}>{"Stop"}</button>
                    <button type="button" class={disabled_button.clone() + " is-success"}>{"Add all breakpoints"}</button>
                    <button type="button" class={disabled_button + " is-error"}>{"Remove all breakpoints"}</button>
                </>
            }
        } else {
            html! {
                <>
                    <button type="button" class={enabled_button.clone()} onclick={ctx.link().callback(|_| Message::Run)}>{"Run"}</button>
                    <button type="button" class={disabled_button.clone() + " is-primary"}>{"Continue"}</button>
                    <button type="button" class={disabled_button.clone() + " is-warning"}>{"Stop"}</button>
                    <button type="button" class={enabled_button.clone() + " is-success"} onclick={ctx.link().callback(|_| Message::AddAllBreakpoints)}>{"Add all breakpoints"}</button>
                    <button type="button" class={enabled_button + " is-error"} onclick={ctx.link().callback(|_| Message::RemoveAllBreakpoints)}>{"Remove all breakpoints"}</button>
                </>
            }
        };
        html! {
            <>
            <div class="controls nes-container with-title" style={style}>
                <h3 class="title">{"Controls"}</h3>
                <div class="half">
                    {self.rule_run(ctx)}
                    <br/>
                    {self.breakpoints(ctx)}
                </div>
                {buttons}

            </div>
            </>
        }
    }

    fn header(&self) -> Html {
        html! {
            <header class="{ sticky: scrollPos > 50 }">
                <div class="container">
                    <div class="nav-brand">
                    <h1><img src="https://raw.githubusercontent.com/sbeckeriv/pest_format/master/docs/logo.gif" height="50"/>{" pest web debugger"}</h1>
                    </div>
                </div>
            </header>
        }
    }

    fn error_dialog(&self) -> Html {
        if let Some(err) = &self.state.error {
            html! {
            <dialog class="nes-dialog" id="dialog-default" ref={self.modal_ref.clone()}>
                <form method="dialog">
                <p class="title">{"Error"}</p>
                <pre>{err}</pre>
                <menu class="dialog-menu">
                    <button class="nes-btn">{"Close"}</button>
                </menu>
                </form>
            </dialog>
            }
        } else {
            html!()
        }
    }

    fn rule_run(&self, ctx: &Context<Self>) -> Html {
        let options = self.state.breakpoints.iter().map(|(_b, r)| {
            if r == &self.state.to_run {
                html! {
                    <option value={r.clone()} selected={true} disabled={self.state.running}>{r}</option>
                }
            } else {
                html! {
                    <option value={r.clone()} disabled={self.state.running}>{r}</option>
                }
            }
        }).collect::<Html>();
        html! {
            <>
            <label for="rule_run">{"Select a rule to run"}</label>
            <div class="nes-select" onchange={ctx.link().callback(Message::SelectRuleToRun)}>
            <select id="rule_run">
                {options}
            </select>
            </div>
            </>
        }
    }

    fn breakpoints(&self, ctx: &Context<Self>) -> Html {
        let options = self.state.breakpoints.iter().map(|(b, r)| {
            let event = self.state.events.front();
            let class = match event {
                Some(DebuggerEvent::Breakpoint(rule, ..)) => {
                    if rule == r {
                        "nes-text is-primary"
                    } else {
                        "nes-text"
                    }
                },
                _ => "nes-text",
            };
            html!{
                <>
                <label>
                    <input type="checkbox" class="nes-checkbox" checked={*b} name={r.clone()} onchange={ctx.link().callback(Message::ChangeBreakpoint)} disabled={self.state.running} />
                    <span class={class}>{r}</span>
                </label>
                <br/>
                </>
            }
        }).collect::<Html>();
        html! {
            <>
            <label for="breakpoints">{"Breakpoints"}</label>
            <div id="breakpoints">
                {options}
            </div>
            </>
        }
    }

    fn footer(&self) -> Html {
        html! {
            <div id="footer" style="clear:both; width: 62%; margin:20px">
                <section class="nes-container with-title">
                <h3 class="title">{"Thanks"}</h3>
                <section class="message-list">
                <section class="message -left">
                <i class="nes-ash animate is-small"></i>
                <div class="nes-balloon from-left">
                <p>{"Thanks to "} <a href="https://pest.rs/" target="_blank">{"pest"}</a> <br/> {" and "} <a href="https://docs.rs/pest_debugger/2.5.7/pest_debugger/" target="_blank">{ "pest_debugger" }</a> {" (well)"}</p>
                </div>
                </section>
                <section class="message -right">
                <div class="nes-balloon from-right">
                <p><a href="https://github.com/tomtau/pest-web-debug" target="_blank">{ "Github repo" }</a></p>
                </div>
                <i class="nes-octocat is-small"></i>
                </section>

                <section class="message -left">
                <i class="nes-ash animate is-small"></i>
                <div class="nes-balloon from-left">
                <p><a href="https://nostalgic-css.github.io/NES.css/" target="_blank">{"NES.css"}</a>{", "}<br /> <a href="https://github.com/sbeckeriv/pest_format" target="_blank">{ "sbeckeriv's pest_format layout" }</a><br />{"and "} <a href="https://github.com/yewstack/yew" target="_blank">{ "yew" }</a></p>
                </div>
                </section>
                </section>
                </section>
                </div>
        }
    }
}

impl Component for App {
    type Message = Message;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let cb = {
            let link = ctx.link().clone();
            move |e| link.send_message(Self::Message::WorkerMsg(e))
        };
        let mut worker = Worker::bridge(Rc::new(cb));
        let state = AppState::default();
        worker.send(WorkerInput::LoadGrammar(state.grammar.clone()));
        worker.send(WorkerInput::LoadInput(state.input.clone()));
        Self {
            grammar_ref: NodeRef::default(),
            input_ref: NodeRef::default(),
            modal_ref: NodeRef::default(),
            worker,
            state,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Self::Message::GrammarChange => {
                if let Some(input) = self.grammar_ref.cast::<HtmlTextAreaElement>() {
                    self.state.grammar = input.value();
                    self.worker
                        .send(WorkerInput::LoadGrammar(self.state.grammar.clone()));
                }
                true
            }
            Self::Message::InputChange => {
                if let Some(input) = self.input_ref.cast::<HtmlTextAreaElement>() {
                    self.state.input = input.value();
                    self.worker
                        .send(WorkerInput::LoadInput(self.state.input.clone()));
                }
                true
            }
            Self::Message::SelectRuleToRun(e) => {
                if let Ok(input) = e.target().unwrap().dyn_into::<HtmlSelectElement>() {
                    self.state.to_run = self.state.breakpoints[input.selected_index() as usize]
                        .1
                        .clone();
                }
                true
            }
            Self::Message::ChangeBreakpoint(e) => {
                if let Ok(input) = e.target().unwrap().dyn_into::<HtmlInputElement>() {
                    let rule = input.name();
                    if let Some(index) =
                        self.state.breakpoints.iter().position(|(_b, r)| r == &rule)
                    {
                        self.state.breakpoints[index].0 = input.checked();
                    }
                    if input.checked() {
                        self.worker.send(WorkerInput::AddBreakpoint(rule));
                    } else {
                        self.worker.send(WorkerInput::DeleteBreakpoint(rule));
                    }
                }
                true
            }
            Self::Message::AddAllBreakpoints => {
                self.state.breakpoints = self
                    .state
                    .breakpoints
                    .iter()
                    .map(|x| (true, x.1.clone()))
                    .collect();
                self.worker.send(WorkerInput::AddAllRulesBreakpoints);
                true
            }
            Self::Message::RemoveAllBreakpoints => {
                self.state.breakpoints = self
                    .state
                    .breakpoints
                    .iter()
                    .map(|x| (false, x.1.clone()))
                    .collect();
                self.worker.send(WorkerInput::DeleteAllBreakpoints);
                true
            }
            Self::Message::Run => {
                if self.state.error.is_none() {
                    self.state.running = true;
                    self.worker
                        .send(WorkerInput::Run(self.state.to_run.clone()));
                } else if let Some(input) = self.modal_ref.cast::<HtmlDialogElement>() {
                    let _ = input.show_modal();
                }
                true
            }
            Self::Message::WorkerMsg(msg) => {
                match msg {
                    DebuggerEvent::Rules(rules) => {
                        self.state.breakpoints = rules.iter().map(|x| (false, x.clone())).collect();
                        self.state.error = None;
                    }
                    DebuggerEvent::Error(e) => {
                        self.state.error = Some(e);
                    }
                    _ => {
                        self.state.events.push_back(msg);
                    }
                }
                true
            }
            Self::Message::Continue => {
                if !self.state.events.is_empty() {
                    self.state.events.pop_front();
                    match self.state.events.get(0) {
                        Some(DebuggerEvent::Eof) | None => {
                            self.state.events.pop_front();
                            self.state.running = false;
                        }
                        _ => {}
                    }
                }
                true
            }
            Self::Message::Stop => {
                self.state.running = false;
                self.state.events.clear();
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <>
                <div id="nescss">
                    {self.header()}
                    {self.error_dialog()}
                    <div class="half">
                        <label for="grammar">{"Grammar"}</label>
                        <textarea id="grammar" class="grammar nes-textarea" rows="20" cols="33"
                        ref={self.grammar_ref.clone()} value={self.state.grammar.clone()} oninput={ctx.link().callback(|_| Message::GrammarChange)} readonly={self.state.running}>
                        </textarea>
                    </div>
                    {self.input_display(ctx)}

                    {self.controls(ctx)}
                    <br/>
                    {self.footer()}
                </div>
        </>

        }
    }
}
