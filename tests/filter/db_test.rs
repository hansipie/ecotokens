use ecotokens::filter::db::filter_db;

#[test]
fn psql_table_format_is_compacted() {
    let input = " id | name  | age \n\
----+-------+-----\n\
  1 | Alice |  30 \n\
  2 | Bob   |  25 \n\
(2 rows)\n";
    let out = filter_db(input);
    assert!(out.contains("Alice"), "data should be kept");
    assert!(!out.contains("---"), "separators should be removed");
    assert!(!out.contains("(2 rows)"), "footer should be removed");
}

#[test]
fn psql_table_limits_rows() {
    let mut input = String::from(" id | value \n----+-------\n");
    for i in 0..50 {
        input.push_str(&format!("  {} | data{} \n", i, i));
    }
    input.push_str("(50 rows)\n");
    let out = filter_db(&input);
    assert!(out.contains("[ecotokens]"), "should have truncation marker");
    assert!(out.contains("omitted"), "should say rows omitted");
}

#[test]
fn psql_expanded_format_is_parsed() {
    let input = "-[ RECORD 1 ]----\n\
id   | 1\n\
name | Alice\n\
-[ RECORD 2 ]----\n\
id   | 2\n\
name | Bob\n";
    let out = filter_db(input);
    assert!(out.contains("id = 1"), "expanded field should be formatted");
    assert!(out.contains("name = Alice"), "expanded field should be kept");
}

#[test]
fn psql_non_table_passes_through() {
    let input = "INSERT 0 1\n";
    let out = filter_db(input);
    assert_eq!(out, input, "non-table output should pass through");
}
