//! A /usr/bin/tee clone
//!
//! Copies standard input to standard output, and also to zero or more files.
//!
//! Supports standard tee options -a and -i.
//!
//!     -a, --append               append to FILEs, do not overwrite
//!     -i, --ignore-interrupts    ignore interrupt signals (ctrl+c)

#![deny(warnings)]

extern crate clap;
extern crate futures;
extern crate tokio;
extern crate tokio_signal;

use futures::sink::BoxSink;
use futures::{Future, Sink, Stream};
use tokio::codec::{BytesCodec, FramedRead, FramedWrite};

fn main() -> Result<(), Box<std::error::Error>> {
    let matches = clap::App::new("tee")
        .about("copy stdin to each FILE, and also to stdout")
        .arg(
            clap::Arg::with_name("FILE")
                .multiple(true)
                .help("output file path(s)"),
        )
        .arg(
            clap::Arg::with_name("append")
                .short("a")
                .long("append")
                .help("append to FILEs, do not overwrite"),
        )
        .arg(
            clap::Arg::with_name("ignore-interrupts")
                .short("i")
                .long("ignore-interrupts")
                .help("ignore interrupt signals (ctrl+c)"),
        )
        .get_matches();

    let files: Vec<String> = match matches.values_of("FILE") {
        Some(files) => files.map(|file| file.into()).collect(),
        None => vec![],
    };
    let append = matches.is_present("append");
    let ignore_interrupts = matches.is_present("ignore-interrupts");

    let in_stream =
        FramedRead::new(tokio::io::stdin(), BytesCodec::new()).map(|bytes_mut| bytes_mut.freeze());
    let stdout_sink = Box::new(FramedWrite::new(tokio::io::stdout(), BytesCodec::new()));

    let starter = futures::stream::iter_ok::<_, ()>(files)
        .map(move |file| {
            let f_sink = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(file.clone())
                .and_then(|f| {
                    let f_sink = FramedWrite::new(f, BytesCodec::new());
                    futures::future::ok(f_sink)
                })
                .map_err(move |e| {
                    eprintln!("{}: {}", file, e);
                    std::process::exit(1);
                });

            f_sink
        })
        .and_then(|f_sink| f_sink)
        .fold::<_, BoxSink<_, _>, _>(stdout_sink, |fanout_sink, f_sink| {
            let result = fanout_sink.fanout(f_sink);
            futures::future::ok(Box::new(result) as BoxSink<_, _>)
        })
        .map(move |fanout_sink| {
            let tee_pipe = fanout_sink
                .send_all(in_stream)
                .map(|_| ()) // input finished
                .map_err(|e| {
                    // not sure when this error can happen
                    eprintln!("{}", e);
                    std::process::exit(1);
                });
            tokio::spawn(tee_pipe);

            if ignore_interrupts {
                let sigint_ignorer = tokio_signal::ctrl_c()
                    .flatten_stream()
                    .for_each(|()| Ok(()))
                    .map_err(|e| {
                        // not sure when this error can happen either
                        eprintln!("ctrl+c error? {}", e);
                        std::process::exit(1);
                    });
                tokio::spawn(sigint_ignorer);
            };
        });

    tokio::run(starter);

    Ok(())
}
