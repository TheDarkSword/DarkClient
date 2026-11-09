package net.minecraft.world.entity;

import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.ItemStack;

/** Holds an item in the main hand only. */
public class LivingEntity extends Entity {
    public ItemStack getItemInHand(InteractionHand hand) {
        return hand == InteractionHand.MAIN_HAND ? new ItemStack(false) : ItemStack.EMPTY;
    }
}
