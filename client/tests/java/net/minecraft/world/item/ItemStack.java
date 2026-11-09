package net.minecraft.world.item;

public class ItemStack {
    public static final ItemStack EMPTY = new ItemStack(true);
    private final boolean empty;

    public ItemStack(boolean empty) {
        this.empty = empty;
    }

    public boolean isEmpty() {
        return empty;
    }
}
