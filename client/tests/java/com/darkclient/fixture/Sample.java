package com.darkclient.fixture;

/**
 * A plain class with overloads and varied parameter types, used to exercise
 * the reflected mapping path against a real JVM.
 */
public class Sample {
    public int counter = 7;

    public static Sample create() {
        return new Sample();
    }

    public int value(int n) {
        return n * 2;
    }

    public double value(double n) {
        return n * 2.0;
    }

    public String greet(String who) {
        return "hi " + who;
    }

    public long sum(int a, long b) {
        return a + b;
    }

    public void fail() {
        throw new IllegalStateException("fixture failure");
    }
}
