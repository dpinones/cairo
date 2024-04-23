use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Error, Result};
use cairo_lang_compiler::diagnostics::get_diagnostics_as_string;
use cairo_lang_compiler::{
    db::RootDatabase, diagnostics::DiagnosticsReporter,
    wasm_cairo_interface::setup_project_with_input_string,
};
use cairo_lang_diagnostics::ToOption;
use cairo_lang_filesystem::log_db::LogDatabase;
use cairo_lang_sierra_generator::db::SierraGenGroup;
use cairo_lang_sierra_generator::program_generator::SierraProgramWithDebug;
use cairo_lang_sierra_generator::replace_ids::{DebugReplacer, SierraIdReplacer};
use cairo_lang_starknet::contract::get_contracts_info;

use crate::profiling::ProfilingInfoProcessor;
use crate::short_string::as_cairo_short_string;
use crate::{RunResultValue, ProfilingInfoCollectionConfig, SierraCasmRunner, StarknetState, RunResultStarknet};

pub fn run_with_input_program_string(
    input_program_string: &String,
    available_gas: Option<usize>,
    allow_warnings: bool,
    print_full_memory: bool,
    run_profiler: bool,
    use_dbg_print_hint: bool,
) -> Result<String> {
    let path = Path::new("astro.cairo");

    let mut db_builder = RootDatabase::builder();
    db_builder.detect_corelib();
    if available_gas.is_none() {
        db_builder.skip_auto_withdraw_gas();
    }
    let db = &mut db_builder.build()?;

    let main_crate_ids = setup_project_with_input_string(db, path, &input_program_string)?;

    let mut reporter = DiagnosticsReporter::stderr();
    if allow_warnings {
        reporter = reporter.allow_warnings();
    }

    /*
    if reporter.check(db) {
        anyhow::bail!("failed to compile: {}", path.display());
    }
     */

     if reporter.check(db) {
        let err_string = get_diagnostics_as_string(db, &[]);
        anyhow::bail!("failed to compile 2:\n {}", err_string);
    }
    
    let SierraProgramWithDebug { program: sierra_program, debug_info } = Arc::unwrap_or_clone(
        db.get_sierra_program(main_crate_ids.clone())
            .to_option()
            .with_context(|| "Compilation failed without any diagnostics.")?,
    );
    let replacer = DebugReplacer { db };
    if available_gas.is_none() && sierra_program.requires_gas_counter() {
        anyhow::bail!("Program requires gas counter, please provide `--available-gas` argument.");
    }

    let contracts_info = get_contracts_info(db, main_crate_ids, &replacer)?;
    let sierra_program = replacer.apply(&sierra_program);

    let runner = SierraCasmRunner::new(
        sierra_program.clone(),
        if available_gas.is_some() { Some(Default::default()) } else { None },
        contracts_info,
        if run_profiler { Some(ProfilingInfoCollectionConfig::default()) } else { None },
    )
    // .with_context(|| "Failed setting up runner.")?;
    .map_err(|err| Error::msg(err.to_string()))?;

    let result = runner
        .run_function_with_starknet_context(
            runner.find_function("::main").map_err(|err| Error::msg(err.to_string()))?,
            &[],
            available_gas,
            StarknetState::default(),
        )
        // .with_context(|| "Failed to run the function.")?;
        .map_err(|err| Error::msg(err.to_string()))?;
    
    /*
    if args.run_profiler {
        let profiling_info_processor = ProfilingInfoProcessor::new(
            Some(db),
            sierra_program,
            debug_info.statements_locations.get_statements_functions_map_for_tests(db),
        );
        match result.profiling_info {
            Some(raw_profiling_info) => {
                let profiling_info = profiling_info_processor.process(&raw_profiling_info);
                println!("Profiling info:\n{}", profiling_info);
            }
            None => println!("Warning: Profiling info not found."),
        }
    }
     */
    generate_run_result_log(&result, print_full_memory, use_dbg_print_hint)
}

fn generate_run_result_log(
    result: &RunResultStarknet,
    print_full_memory: bool,
    use_dbg_print_hint: bool,
) -> Result<String> {
    let mut result_string = String::new();

    if use_dbg_print_hint {
        println!("{}\n", LogDatabase::get_file_text("log_file".to_string()));
        result_string.push_str(&format!("{}", LogDatabase::get_file_text("log_file".to_string())));
    }

    match &result.value {
        RunResultValue::Success(values) => {
            println!("Run completed successfully, returning {values:?}");
            result_string.push_str(&format!(
                "Run completed successfully, returning {values:?}\n",
                values = values
            ))
        }
        RunResultValue::Panic(values) => {
            print!("Run panicked with [");
            result_string.push_str(&format!("Run panicked with ["));
            for value in values {
                match as_cairo_short_string(value) {
                    Some(as_string) => {
                        print!("{value} ('{as_string}'), ");
                        result_string.push_str(&format!(
                            "{value} ('{as_string}'), ",
                            value = value,
                            as_string = as_string
                        ));
                    }
                    None => {
                        print!("{value}, ");
                        result_string.push_str(&format!("{value}, ", value = value))
                    }
                }
            }
            println!("].");
            result_string.push_str(&format!("].\n"))
        }
    }
    if let Some(gas) = &result.gas_counter {
        println!("Remaining gas: {gas}");
        result_string.push_str(&format!("Remaining gas: {gas}\n", gas = gas));
    }
    if print_full_memory {
        print!("Full memory: [");
        result_string.push_str(&format!("Full memory: ["));
        for cell in &result.memory {
            match cell {
                None => {
                    print!("_, ");
                    result_string.push_str(&format!("_, "))
                }
                Some(value) => {
                    print!("{value}, ");
                    result_string.push_str(&format!("{value}, ", value = value))
                }
            }
        }
        result_string.push_str(&format!("]\n"))
    }
    Ok(result_string)
}
