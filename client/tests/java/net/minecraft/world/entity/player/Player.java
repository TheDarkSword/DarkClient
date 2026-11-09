package net.minecraft.world.entity.player;

import net.minecraft.world.entity.LivingEntity;

/** Test stand-in for Minecraft's Player entity. */
public class Player extends LivingEntity {
    private final Abilities abilities = new Abilities();

    public Abilities getAbilities() {
        return abilities;
    }
}
