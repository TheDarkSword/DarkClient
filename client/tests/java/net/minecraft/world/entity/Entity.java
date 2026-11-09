package net.minecraft.world.entity;

import net.minecraft.world.phys.Vec3;

public class Entity {
    private static int nextId;
    private final int id = ++nextId;

    public int getId() {
        return id;
    }

    public Vec3 position() {
        return new Vec3(1.5, 64.0, -2.25);
    }
}
