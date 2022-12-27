use debug::DebugWithDb;
use itertools::Itertools;
use lowering::db::LoweringGroup;
use pretty_assertions::assert_eq;
use semantic::test_utils::setup_test_function;
use utils::ordered_hash_map::OrderedHashMap;

use super::find_variable_lifetime;
use crate::test_utils::SierraGenDatabaseForTesting;

test_utils::test_file_test!(
    variable_lifetime,
    [
        "src/lifetime_test_data/simple",
        "src/lifetime_test_data/block",
        "src/lifetime_test_data/struct",
    ],
    SierraGenDatabaseForTesting,
    check_variable_lifetime
);

fn check_variable_lifetime(
    db: &mut SierraGenDatabaseForTesting,
    inputs: &OrderedHashMap<String, String>,
) -> OrderedHashMap<String, String> {
    // Parse code and create semantic model.
    let test_function = setup_test_function(
        db,
        inputs["function_code"].as_str(),
        inputs["function_name"].as_str(),
        inputs["module_code"].as_str(),
    )
    .unwrap();

    db.module_lowering_diagnostics(test_function.module_id)
        .unwrap()
        .expect_with_db(db, "Unexpected diagnostics.");

    let lowered_function = &*db.free_function_lowered(test_function.function_id).unwrap();

    let lowered_formatter = lowering::fmt::LoweredFormatter { db, lowered: lowered_function };
    let lowered_str = format!("{:?}", lowered_function.debug(&lowered_formatter));

    let find_variable_lifetime_res = find_variable_lifetime(lowered_function)
        .expect("find_variable_lifetime failed unexpectedly");
    let last_use_str = find_variable_lifetime_res
        .last_use
        .iter()
        .map(|(var_id, location)| format!("{:?}: {location:?}", var_id.debug(&lowered_formatter)))
        .join("\n");
    let drop_str = find_variable_lifetime_res
        .drops
        .iter()
        .map(|(var_id, location)| format!("{:?}: {location:?}", var_id.debug(&lowered_formatter)))
        .join("\n");

    OrderedHashMap::from([
        ("lowering_format".into(), lowered_str),
        ("last_use".into(), last_use_str),
        ("drops".into(), drop_str),
    ])
}