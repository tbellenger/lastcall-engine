fn main() {
    if let Err(err) = run() {
        eprintln!("advc: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or_else(usage)?;
    match command.as_str() {
        "validate" => {
            let dir = args.next().ok_or_else(usage)?;
            advc::validate_dir(dir.as_ref()).map_err(|err| err.to_string())?;
            println!("valid {dir}");
        }
        "pack" => {
            let dir = args.next().ok_or_else(usage)?;
            let mut output = None;
            while let Some(flag) = args.next() {
                if flag == "-o" {
                    output = Some(args.next().ok_or_else(usage)?);
                } else {
                    return Err(usage());
                }
            }
            let output = output.ok_or_else(usage)?;
            advc::pack_dir(dir.as_ref(), output.as_ref()).map_err(|err| err.to_string())?;
            println!("packed {output}");
        }
        "test" => {
            let dir = args.next().ok_or_else(usage)?;
            let mut update = false;
            for flag in args {
                if flag == "--update-snapshots" {
                    update = true;
                } else {
                    return Err(usage());
                }
            }
            advc::test_dir(dir.as_ref(), update).map_err(|err| err.to_string())?;
            println!("passed {dir}");
        }
        "-h" | "--help" => println!("{}", usage()),
        other => return Err(format!("unknown command {other}\n{}", usage())),
    }
    Ok(())
}

fn usage() -> String {
    "usage:\n  advc validate <game-dir>\n  advc pack <game-dir> -o <file.advpak>\n  advc test <game-dir> [--update-snapshots]"
        .into()
}
