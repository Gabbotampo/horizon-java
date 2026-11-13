package demo;

public final class Probe {
    static final StringBuilder LOG = new StringBuilder();

    static class Lazy {
        static { LOG.append("lazy;"); }
    }

    static class Eager {
        static { LOG.append("eager;"); }
    }

    static class Broken {
        static int value = fail();

        static int fail() {
            throw new IllegalStateException("boom");
        }
    }

    public static String run() throws Exception {
        Class.forName("demo.Probe$Lazy", false, Probe.class.getClassLoader());
        LOG.append("after-lazy;");
        Class.forName("demo.Probe$Eager");
        LOG.append("after-eager;");
        Class.forName("demo.Probe$Eager");
        try {
            Class.forName("demo.Probe$Broken");
        } catch (ExceptionInInitializerError error) {
            LOG.append("eiie;");
        }
        try {
            Class.forName("demo.Probe$Broken");
        } catch (NoClassDefFoundError error) {
            LOG.append("ncdfe;");
        }
        new Lazy();
        return LOG.toString();
    }

}
