use runmdl::runtime;
use std::sync::mpsc;
use structopt::StructOpt;

mod debugger;

#[derive(StructOpt)]
struct Arguments {
    /// Path to the program to run.
    #[structopt(long, short)]
    program: std::path::PathBuf,
    #[structopt(long)]
    interactive: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let all_arguments: Vec<String> = std::env::args().collect();
    let application_arguments_start = all_arguments.iter().position(|arg| arg == "--");

    let interpreter_arguments = Arguments::from_iter(
        &all_arguments[0..application_arguments_start.unwrap_or(all_arguments.len())],
    );

    let application_arguments = application_arguments_start
        .map(|start| {
            all_arguments[start + 1..all_arguments.len()]
                .iter()
                .map(String::as_str)
                .collect()
        })
        .unwrap_or(Vec::default());

    let application =
        registir::parser::parse_module(&mut std::fs::File::open(interpreter_arguments.program)?)?;

    let mut initializer = runtime::Initializer::new();
    let runtime = runtime::Runtime::initialize(&mut initializer, application);

    let (debugger, debugger_channel_receiver) = if interpreter_arguments.interactive {
        let (debugger_channel_sender, debugger_channel_receiver) = mpsc::sync_channel(0);
        let debugger_thread = std::thread::Builder::new()
            .name("debugger".to_string())
            .spawn(|| debugger::start(debugger_channel_sender))?;
        (Some(debugger_thread), Some(debugger_channel_receiver))
    } else {
        (None, None)
    };

    let exit_code =
        runtime.invoke_entry_point(application_arguments.as_ref(), debugger_channel_receiver)?;

    if let Some(debugger_thread) = debugger {
        debugger_thread.join().unwrap();
    }

    std::process::exit(exit_code)
}
