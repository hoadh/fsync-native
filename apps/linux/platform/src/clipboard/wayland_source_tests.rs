use super::*;
use std::io::Write;

#[test]
fn reads_to_eof_and_enforces_the_cap() {
    let (r, w) = make_pipe().expect("pipe");
    let mut w = std::fs::File::from(w);
    w.write_all(b"hello").expect("write");
    drop(w);
    assert_eq!(
        read_pipe(r, 100, Duration::from_secs(1)),
        Ok(b"hello".to_vec())
    );
    let (r, w) = make_pipe().expect("pipe");
    let mut w = std::fs::File::from(w);
    w.write_all(&[1u8; 2000]).expect("write");
    drop(w);
    assert_eq!(
        read_pipe(r, 1000, Duration::from_secs(1)),
        Err(ReadError::TooLarge)
    );
}

#[test]
fn a_silent_source_times_out() {
    let (r, _w) = make_pipe().expect("pipe");
    let t = Instant::now();
    assert_eq!(
        read_pipe(r, 10, Duration::from_millis(200)),
        Err(ReadError::Timeout)
    );
    assert!(t.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_stalled_reader_does_not_block_a_stopping_writer() {
    let (_r, w) = make_pipe().expect("pipe");
    let big = vec![7u8; 4 << 20]; // far beyond the pipe buffer, nobody reads
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = Arc::clone(&stop);
    let h = std::thread::spawn(move || write_bounded(&w, &big, &s2, Duration::from_secs(60)));
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    stop.store(true, Ordering::SeqCst);
    assert!(!h.join().expect("join").expect("io"));
    assert!(t.elapsed() < Duration::from_secs(1));
}

#[test]
fn writer_gate_bounds_and_waits() {
    let g = Arc::new(WriterGate::default());
    let slots: Vec<_> = (0..MAX_WRITERS).map(|_| g.enter().expect("slot")).collect();
    assert!(g.enter().is_none());
    assert!(!g.wait_idle(Duration::from_millis(50)));
    drop(slots);
    assert!(g.wait_idle(Duration::from_millis(50)));
}
