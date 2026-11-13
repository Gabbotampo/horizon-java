package demo;

import java.util.Properties;

public final class Probe {
    static String read(String name) throws Exception {
        java.io.InputStream in = Probe.class.getResourceAsStream(name);
        if (in == null) {
            return "absent";
        }
        Properties properties = new Properties();
        properties.load(in);
        return properties.getProperty("from");
    }

    public static String run() throws Exception {
        return read("/lib-defaults.properties") + "|" + read("/shared.properties") + "|"
            + read("/missing.properties") + "|" + (Probe.class.getResource("/lib-defaults.properties") != null);
    }

}
