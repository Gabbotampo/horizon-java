package demo;

import net.fabricmc.loader.api.FabricLoader;
import net.fabricmc.loader.api.ModContainer;

public final class Probe {
    public static String ids() {
        StringBuilder out = new StringBuilder();
        for (ModContainer container : FabricLoader.getInstance().getAllMods()) {
            if (out.length() > 0) {
                out.append(',');
            }
            out.append(container.getMetadata().getId());
        }
        return out.toString();
    }

    public static String describe() {
        StringBuilder out = new StringBuilder();
        for (ModContainer container : FabricLoader.getInstance().getAllMods()) {
            String id = container.getMetadata().getId();
            if (id.equals("alpha") || id.equals("beta") || id.equals("gamma")
                || id.equals("fabricloader") || id.equals("mixinextras")) {
                out.append(Describe.line(container.getMetadata())).append('\n');
            }
        }
        return out.toString();
    }

    /** What the built-in containers know, in list order. */
    public static String builtins() {
        StringBuilder out = new StringBuilder();
        for (ModContainer container : FabricLoader.getInstance().getAllMods()) {
            String id = container.getMetadata().getId();
            if (id.equals("fabricloader")) {
                out.append("loader-name=").append(container.getMetadata().getName()).append(';');
            }
            if (id.equals("java")) {
                out.append("java=").append(container.getMetadata().getName()).append('/')
                    .append(container.getMetadata().getVersion().getFriendlyString()).append('/')
                    .append(container.getMetadata().getType()).append('/')
                    .append(container.getMetadata().containsCustomValue("modmenu")).append(';');
            }
            if (id.equals("minecraft")) {
                out.append("minecraft=").append(container.getMetadata().getName()).append('/')
                    .append(container.getMetadata().getVersion().getFriendlyString()).append('/')
                    .append(container.getMetadata().getEnvironment().name()).append('/')
                    .append(container.getMetadata().getContact().get("homepage").orElse("-")).append(';');
            }
        }
        return out.toString();
    }
}
