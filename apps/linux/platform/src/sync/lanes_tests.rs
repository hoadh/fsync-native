use super::*;
use std::sync::mpsc;

#[test]
fn newest_wins_while_busy_and_the_running_job_is_cancelled() {
    let (tx, rx) = mpsc::channel();
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let gate_rx = Mutex::new(gate_rx);
    let lane = Lane::spawn("t", move |n: u32, c: Cancel| {
        if n == 1 {
            gate_rx.lock().expect("gate").recv().expect("gate open");
        }
        tx.send((n, c.is_cancelled())).expect("send");
    })
    .expect("lane");
    lane.submit(1);
    std::thread::sleep(Duration::from_millis(100));
    lane.submit(2);
    lane.submit(3); // replaces 2
    gate_tx.send(()).expect("open");
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2)).expect("first"),
        (1, true)
    );
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2)).expect("second"),
        (3, false)
    );
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "2 was dropped"
    );
    assert!(lane.shutdown(Duration::from_secs(1)));
}

#[test]
fn a_panicking_job_does_not_kill_the_lane_and_shutdown_is_bounded() {
    let (tx, rx) = mpsc::channel();
    let lane = Lane::spawn("p", move |n: u32, _| {
        assert!(n != 0, "boom");
        tx.send(n).expect("send");
    })
    .expect("lane");
    lane.submit(0);
    std::thread::sleep(Duration::from_millis(100));
    lane.submit(7);
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2))
            .expect("after panic"),
        7
    );
    assert!(lane.shutdown(Duration::from_secs(1)));
    lane.submit(9);
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "no jobs after shutdown"
    );
}

#[test]
fn shutdown_gives_up_on_a_stuck_job() {
    let lane =
        Lane::spawn("s", |_: u32, _| std::thread::sleep(Duration::from_secs(3))).expect("lane");
    lane.submit(1);
    std::thread::sleep(Duration::from_millis(100));
    let t = Instant::now();
    assert!(!lane.shutdown(Duration::from_millis(300)));
    assert!(t.elapsed() < Duration::from_secs(1));
}
