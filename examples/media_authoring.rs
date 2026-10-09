//! Standalone media example. Run with `--features media`.
#[cfg(feature = "media")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use msi::{Column, Insert, Package, PackageType, Value, media};
    use std::io::Cursor;
    let files =
        [("Example", "example.txt", b"Standalone MSI media".as_slice())];
    let media = media::prepare(
        &files,
        &media::Layout::Embedded,
        "Example",
        &media::Limits::default(),
    )?;
    let mut package =
        Package::create(PackageType::Installer, Cursor::new(Vec::new()))?;
    package.create_table(
        "Media",
        vec![
            Column::build("DiskId").primary_key().int16(),
            Column::build("LastSequence").int32(),
            Column::build("Cabinet").nullable().string(255),
        ],
    )?;
    package.create_table(
        "File",
        vec![
            Column::build("File").primary_key().string(72),
            Column::build("FileName").string(255),
            Column::build("FileSize").int32(),
            Column::build("Attributes").int16(),
            Column::build("Sequence").int32(),
        ],
    )?;
    for (index, (last_sequence, cabinet)) in media.rows().iter().enumerate() {
        package.insert_rows(Insert::into("Media").row(vec![
                Value::Int(index as i32 + 1),
                Value::Int(*last_sequence),
                cabinet
                    .as_ref()
                    .map_or(Value::Null, |name| Value::Str(name.clone())),
            ]))?;
    }
    package.insert_rows(Insert::into("File").row(vec![
        Value::Str("Example".into()),
        Value::Str("example.txt".into()),
        Value::Int(files[0].2.len() as i32),
        Value::Int(0x4000),
        Value::Int(1),
    ]))?;
    let flags = package.summary_info().word_count().unwrap_or(0);
    package
        .summary_info_mut()
        .set_word_count(media::summary_word_count(flags, media.compressed()));
    media.write_embedded(&mut package)?;
    package.flush()?;
    println!(
        "Prepared {} media rows and {} embedded cabinets",
        media.rows().len(),
        media.embedded().len()
    );
    Ok(())
}
#[cfg(not(feature = "media"))]
fn main() {
    eprintln!("Run with --features media");
}
