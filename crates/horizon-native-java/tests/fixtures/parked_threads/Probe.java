package demo;

public final class Probe {
    static boolean ran;

    static final class Worker implements Runnable {
        @Override
        public void run() {
            ran = true;
        }
    }

    public static String run() {
        new Thread(new Worker(), "watcher-1").start();
        new Thread(() -> ran = true).start();
        new Thread("named-only").start();
        // Made but never started: nothing to report.
        new Thread(new Worker(), "idle");
        return String.valueOf(ran);
    }
}
