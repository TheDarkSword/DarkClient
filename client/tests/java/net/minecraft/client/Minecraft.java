package net.minecraft.client;

import com.mojang.blaze3d.platform.Window;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.multiplayer.MultiPlayerGameMode;
import net.minecraft.client.player.LocalPlayer;

/**
 * Test stand-in for Minecraft's client class.
 *
 * The static instance always exists (the game window is up); the
 * world-scoped fields are null until {@link #enterWorld()} is called, which
 * mirrors being in the main menu versus being in a world.
 */
public class Minecraft {
    private static final Minecraft INSTANCE = new Minecraft();

    public LocalPlayer player;
    public ClientLevel level;
    public MultiPlayerGameMode gameMode;
    public Screen screen;
    public final Gui gui = new Gui();
    private final Window window = new Window();

    public static Minecraft getInstance() {
        return INSTANCE;
    }

    public Window getWindow() {
        return window;
    }

    /** Simulates joining a world. */
    public static void enterWorld() {
        INSTANCE.player = new LocalPlayer();
        INSTANCE.level = new ClientLevel();
        INSTANCE.gameMode = new MultiPlayerGameMode();
    }

    /** Simulates leaving a world. */
    public static void leaveWorld() {
        INSTANCE.player = null;
        INSTANCE.level = null;
        INSTANCE.gameMode = null;
    }
}
