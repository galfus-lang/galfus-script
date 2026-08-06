use galfus_frontend::*;

fn main() {
    let source = galfus_core::SourceFile::new(galfus_core::SourceId::new(0), "test.gfp".to_string(), "struct Window {}\nexport const w = Window\n".to_string());
    let parse_result = parse(&source);
    println!("{:#?}", parse_result.into_ast().syntax());
}
