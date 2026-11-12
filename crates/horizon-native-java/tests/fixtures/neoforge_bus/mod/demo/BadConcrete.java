package demo;

import net.neoforged.bus.api.Event;

/// A concrete event under an abstract one (`BadAbstract`), which NeoForge refuses.
public class BadConcrete extends Event {}
