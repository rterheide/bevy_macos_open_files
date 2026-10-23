//! The Info.plist templates in the README and in the example are what people copy, so they must be valid and say what the README claims.

use plist::Value;

fn xml_block_after(text: &str, start_marker: &str, end_marker: &str) -> String {
    let start = text.find(start_marker).expect("start marker is present") + start_marker.len();
    let end = start
        + text[start..]
            .find(end_marker)
            .expect("end marker is present");
    text[start..end].trim().to_string()
}

fn parse(xml: &str) -> plist::Dictionary {
    Value::from_reader_xml(xml.as_bytes())
        .expect("valid xml property list")
        .into_dictionary()
        .expect("the top level is a dictionary")
}

fn readme_plist() -> plist::Dictionary {
    parse(&xml_block_after(
        include_str!("../README.md"),
        "```xml\n",
        "```",
    ))
}

fn example_plist() -> plist::Dictionary {
    parse(&xml_block_after(
        include_str!("../examples/bundle_example.sh"),
        "<<'PLIST'\n",
        "\nPLIST",
    ))
}

fn url_schemes(plist: &plist::Dictionary) -> Vec<String> {
    plist["CFBundleURLTypes"]
        .as_array()
        .expect("an array")
        .iter()
        .flat_map(|entry| {
            entry.as_dictionary().expect("a dictionary")["CFBundleURLSchemes"]
                .as_array()
                .expect("an array")
                .iter()
                .map(|scheme| scheme.as_string().expect("a string").to_string())
        })
        .collect()
}

#[test]
fn the_readme_template_is_valid_and_registers_files_and_a_scheme() {
    let plist = readme_plist();

    for key in [
        "CFBundleIdentifier",
        "CFBundleExecutable",
        "CFBundlePackageType",
        "CFBundleDocumentTypes",
        "CFBundleURLTypes",
        "LSMinimumSystemVersion",
    ] {
        assert!(plist.contains_key(key), "the README template has no {key}");
    }
    assert_eq!(url_schemes(&plist), vec!["myapp"]);

    let document_type = plist["CFBundleDocumentTypes"].as_array().expect("an array")[0]
        .as_dictionary()
        .expect("a dictionary");
    assert_eq!(
        document_type["CFBundleTypeRole"].as_string(),
        Some("Viewer")
    );
    assert_eq!(
        document_type["LSHandlerRank"].as_string(),
        Some("Alternate")
    );
    let extensions: Vec<_> = document_type["CFBundleTypeExtensions"]
        .as_array()
        .expect("an array")
        .iter()
        .filter_map(Value::as_string)
        .collect();
    assert_eq!(extensions, vec!["xyz"]);
}

#[test]
fn the_example_template_is_valid_and_matches_what_the_readme_says() {
    let plist = example_plist();

    // The README says the example accepts any file, through the public.item type, and this scheme.
    assert_eq!(url_schemes(&plist), vec!["bevy-open-files-example"]);
    assert!(include_str!("../README.md").contains("bevy-open-files-example://"));
    let content_types = plist["CFBundleDocumentTypes"].as_array().expect("an array")[0]
        .as_dictionary()
        .expect("a dictionary")["LSItemContentTypes"]
        .as_array()
        .expect("an array")
        .iter()
        .filter_map(Value::as_string)
        .collect::<Vec<_>>();
    assert_eq!(content_types, vec!["public.item"]);
    assert_eq!(
        plist["CFBundleExecutable"].as_string(),
        Some("OpenFilesExample"),
        "the script copies the binary to this name"
    );
}

#[test]
fn the_bundle_script_copies_the_binary_to_the_name_in_the_plist() {
    let script = include_str!("../examples/bundle_example.sh");
    assert!(script.contains("Contents/MacOS/OpenFilesExample"));
}
