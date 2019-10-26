use std::{env, fs};

fn main() {
    let res: Result<(), Box<dyn std::error::Error>> = env::args_os().skip(1).try_for_each(|file| {
        let mut doc: toml_edit::Document = fs::read_to_string(&file)?.parse()?;
        let table = doc.as_table_mut();
        table.remove("dev-dependencies");
        if let Some(table) = table.entry("target").as_table_mut() {
            // `toml_edit::Table` does not have `.iter_mut()`, so collect keys.
            let keys: Vec<String> = table.iter().map(|(key, _)| key.to_string()).collect();
            for key in keys {
                if let Some(table) = table.entry(&key).as_table_mut() {
                    table.remove("dev-dependencies");
                }
            }
        }
        fs::write(file, doc.to_string_in_original_order())?;
        Ok(())
    });

    if let Err(e) = res {
        eprintln!("{}", e);
        std::process::exit(1)
    }
}
