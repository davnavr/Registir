use runmdl::interpreter::{debugger, Interpreter};

type CommandResult = Result<debugger::Reply, String>;

type CommandLookup = std::collections::HashMap<&'static str, Command>;

#[derive(Clone, Copy)]
struct Command {
    description: &'static str,
    command: &'static dyn Fn(&CommandLookup, &[&str], &mut Interpreter) -> CommandResult,
}

impl Command {
    fn execute(&self, commands: &CommandLookup, arguments: &[&str], interpreter: &mut Interpreter) -> debugger::Reply {
        (self.command)(commands, arguments, interpreter).unwrap_or_else(|error| {
            eprintln!("Error: {}", error);
            debugger::Reply::Wait
        })
    }
}

struct InputCache {
    line_buffer: String,
}

impl InputCache {
    fn read_command(&mut self) -> Option<(&str, Vec<&str>)> {
        // Line and argument buffers are not cached, though this probably doesn't impact performance.
        self.line_buffer.clear();
        std::io::stdin().read_line(&mut self.line_buffer).unwrap();

        let mut arguments = self.line_buffer.split_whitespace();

        arguments.next().map(|command_name| {
            let mut v = {
                let (min_capacity, max_capacity) = arguments.size_hint();
                Vec::with_capacity(max_capacity.unwrap_or(min_capacity))
            };
            v.extend(arguments);
            (command_name, v)
        })
    }
}

pub struct CommandLineDebugger {
    started: bool,
    commands: std::collections::HashMap<&'static str, Command>,
    input_buffer: InputCache,
}

impl CommandLineDebugger {
    pub fn new() -> Self {
        let mut commands = std::collections::HashMap::new();

        macro_rules! command {
            ($name: expr, $description: expr, $command: expr) => {
                commands.insert(
                    $name,
                    Command {
                        description: $description,
                        command: &$command,
                    },
                );
            };
        }

        command!("help", "lists all commands", |commands, _, _| {
            for (name, Command { description, .. }) in commands {
                println!("- {}, {}", name, description);
            }

            Ok(debugger::Reply::Wait)
        });

        command!("detach", "stops debugging and continues execution of the program", |_, _, _| {
            Ok(debugger::Reply::Detach)
        });

        Self {
            started: false,
            commands,
            input_buffer: InputCache {
                line_buffer: String::new(),
            },
        }
    }
}

impl debugger::Debugger for CommandLineDebugger {
    fn inspect(&mut self, interpreter: &mut Interpreter) -> debugger::Reply {
        if !self.started {
            println!("Type 'help' for help, or 'cont' to begin program execution");
            self.started = true;
        }

        if let Some((name, arguments)) = self.input_buffer.read_command() {
            if let Some(command) = self.commands.get(name) {
                return command.execute(&self.commands, &arguments, interpreter);
            } else {
                eprintln!("'{}' is not a valid command", name);
            }
        }
        
        debugger::Reply::Wait
    }
}
