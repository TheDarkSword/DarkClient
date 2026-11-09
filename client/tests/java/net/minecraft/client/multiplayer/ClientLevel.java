package net.minecraft.client.multiplayer;

import java.util.List;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;

/** Test stand-in for Minecraft's client-side world. */
public class ClientLevel {
    private final List<Entity> entities = List.of(new Entity(), new LivingEntity());

    public Iterable<Entity> entitiesForRendering() {
        return entities;
    }
}
