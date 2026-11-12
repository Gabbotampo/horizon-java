package net.neoforged.fml.event.lifecycle;

import net.neoforged.bus.api.Event;
import net.neoforged.fml.event.IModBusEvent;

// Stand-in with loader-12.0.12's shape: abstract, a mod-bus event.
public abstract class ModLifecycleEvent extends Event implements IModBusEvent {}
