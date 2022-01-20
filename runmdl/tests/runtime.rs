use getmdl::loader::{Identifier, Symbol};
use runmdl::interpreter;

mod setup;

#[test]
fn returns_exit_code() {
    setup::initialize_from_str(
        include_str!(r"../../asmdl_cli/samples/return.txtmdl"),
        |_, _| (),
        |_, runtime| assert_eq!(0, runtime.invoke_entry_point(&[], None).unwrap()),
    );
}

#[test]
fn successful_function_symbol_lookup() {
    setup::initialize_from_str(
        r#".module { .name "SymbolTest"; };

.format {
    .major 0;
    .minor 5;
};

.code @code {
    .entry $BLOCK;
    .block $BLOCK () {
        ret;
    };
};

.function @Helper () returns () export {
    .name "unused";
    .body defined @code;
};
"#,
        |_, _| (),
        |_, runtime| {
            assert!(runtime
                .program()
                .lookup_function(Symbol::Owned(Identifier::try_from("Helper").unwrap()))
                .is_some())
        },
    );
}

#[test]
fn breakpoints_are_set_during_pause() {
    #[derive(Default)]
    struct CollectedData {
        value_1: Option<i32>,
        value_2: Option<i32>,
    }

    setup::initialize_from_str(
        r#".module { .name "BreakpointTest"; };

.format { .major 0; .minor 5; };

.code @code {
    .entry $BLOCK;
    .block $BLOCK () {
        %t_ignored = const.i s32 5;
        %t_exit = const.i s32 42;
        ret %t_exit;
    };
};

.function @test () returns (s32) {
    .name "ThisIsAVeryLongTestOfThingsToSeeIfTheyWillFitAndAllThatYouKnow";
    .body defined @code;
};

.entry @test;
"#,
        |program, _| {
            let program_name = program.header.0.identifier.clone();
            let returned_data = std::rc::Rc::new(std::cell::RefCell::new(CollectedData::default()));
            let mut pause_count = 0u8;
            let data = returned_data.clone();
            let debugger = move |interpreter: &mut interpreter::Interpreter| {
                use interpreter::debugger;

                let call_stack = interpreter.call_stack();
                let trace = call_stack.stack_trace();

                let reply = match pause_count {
                    0 => {
                        call_stack
                            .breakpoints_mut()
                            .insert(debugger::Breakpoint::new_owned(
                                debugger::BlockIndex::entry(),
                                1,
                                program_name.clone(),
                                registir::format::Identifier::try_from("test").unwrap(),
                            ));

                        debugger::Reply::Continue
                    }
                    1 => {
                        call_stack
                            .breakpoints_mut()
                            .insert(debugger::Breakpoint::with_symbol(
                                debugger::BlockIndex::entry(),
                                2,
                                trace[0].function().clone(),
                            ));

                        data.borrow_mut().value_1 =
                            Some(i32::try_from(&trace[0].temporary_registers()[0]).unwrap());
                        debugger::Reply::Continue
                    }
                    2.. => {
                        data.borrow_mut().value_2 =
                            Some(i32::try_from(&trace[0].temporary_registers()[1]).unwrap());
                        debugger::Reply::Detach
                    }
                };

                pause_count += 1;
                reply
            };
            (returned_data, debugger)
        },
        |(data, debugger), runtime| {
            assert_eq!(42, runtime.invoke_entry_point(&[], Some(debugger)).unwrap());
            assert_eq!(Some(5), data.borrow().value_1);
            assert_eq!(Some(42), data.borrow().value_2);
        },
    );
}