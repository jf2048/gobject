use futures_util::future::join_all;
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;

mod convert;
use convert::convert;

#[derive(clap::ArgEnum, Clone, Copy, Debug)]
enum Emit {
    Files,
    Stdout,
}

#[derive(clap::Parser, Debug)]
struct Args {
    /// What data to emit and how
    #[clap(long, arg_enum, default_value_t = Emit::Files)]
    emit: Emit,
    /// Backup any modified files.
    #[clap(long)]
    backup: bool,
    #[clap(parse(from_os_str))]
    files: Vec<PathBuf>,
}

#[derive(derive_new::new)]
struct Output {
    path: PathBuf,
    source: String,
    replaced: bool,
}

fn main() {
    let args = clap::Parser::parse();
    let res = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(clone_blockify(args));
    if let Err(errors) = res {
        for err in errors {
            eprintln!("{}", err);
        }
        std::process::exit(1);
    }
}

#[inline]
async fn clone_blockify(args: Args) -> Result<(), Vec<anyhow::Error>> {
    if args.files.is_empty() {
        convert_stdin().await.map_err(|e| vec![e])
    } else {
        let results = join_all(
            args.files
                .into_iter()
                .map(|path| convert_path(path, args.backup)),
        )
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        match args.emit {
            Emit::Stdout => write_stdout(results),
            Emit::Files => write_files(results).await,
        }
    }
}

#[inline]
async fn convert_stdin() -> anyhow::Result<()> {
    let mut old = String::new();
    tokio::io::stdin().read_to_string(&mut old).await?;
    let source = convert(&old)
        .await
        .map_err(|e| ConvertError::new(e, "stdin".into()))?;
    if let Some(source) = source {
        println!("{}", source);
    } else {
        println!("{}", old);
    }
    Ok(())
}

#[inline]
fn write_stdout(results: Vec<anyhow::Result<Output>>) -> Result<(), Vec<anyhow::Error>> {
    let mut errors = vec![];
    for file in results {
        match file {
            Ok(Output { path, source, .. }) => {
                println!("{}\n\n{}", path.display(), source);
            }
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[inline]
async fn write_files(results: Vec<anyhow::Result<Output>>) -> Result<(), Vec<anyhow::Error>> {
    let results = join_all(results.into_iter().map(|file| async {
        let Output {
            path,
            source,
            replaced,
        } = file?;
        if replaced {
            tokio::fs::write(&path, source).await?;
        }
        Ok::<(), anyhow::Error>(())
    }))
    .await;
    let mut errors = vec![];
    for result in results {
        match result {
            Ok(_) => {}
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[derive(derive_new::new, thiserror::Error, Debug)]
#[error("Failed to convert {path}: {source}")]
pub struct ConvertError {
    #[source]
    source: anyhow::Error,
    path: PathBuf,
}

async fn convert_path(path: PathBuf, backup: bool) -> Vec<anyhow::Result<Output>> {
    let meta = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(e) => return vec![Err(ConvertError::new(e.into(), path).into())],
    };
    if meta.is_dir() {
        let tasks = tokio::task::spawn_blocking(|| {
            let mut types = ignore::types::TypesBuilder::new();
            types.add_defaults();
            types.select("rust");
            let mut builder = ignore::WalkBuilder::new(path);
            builder.types(types.build().unwrap());
            let walk = builder.build();
            walk.collect::<Vec<_>>()
        })
        .await
        .unwrap()
        .into_iter()
        .map(|entry| async {
            let entry = entry.map_err(anyhow::Error::from)?;
            convert_file(entry.path(), backup).await
        });
        join_all(tasks).await
    } else {
        vec![convert_file(&path, backup).await]
    }
}

async fn convert_file(path: &Path, backup: bool) -> anyhow::Result<Output> {
    async {
        let old = tokio::fs::read_to_string(path).await?;
        if let Some(source) = convert(&old).await? {
            if backup {
                let backup_path = path.with_extension("bk");
                tokio::fs::write(backup_path, old).await?;
            }
            Ok(Output::new(path.to_owned(), source, true))
        } else {
            Ok(Output::new(path.to_owned(), old, false))
        }
    }
    .await
    .map_err(|e| ConvertError::new(e, path.to_owned()).into())
}
