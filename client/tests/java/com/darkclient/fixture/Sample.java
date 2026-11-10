package com.darkclient.fixture;

/** Exercises the runtime's raw calls, fields and constructors. */
public class Sample {
    public static boolean loudInitialized;
    public int counter = 7;

    public static Sample create() {
        return new Sample();
    }

    public long sum(int a, long b) {
        return a + b;
    }

    public void fail() {
        throw new IllegalStateException("fixture failure");
    }
}
