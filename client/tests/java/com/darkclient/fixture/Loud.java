package com.darkclient.fixture;

/** Its static initializer leaves a mark: the binding self-check must not run it. */
public class Loud extends Sample implements Runnable {
    static {
        Sample.loudInitialized = true;
    }

    public double ratio;

    public Loud(int seed) {}

    public void run() {}
}
