package demo;

import java.util.ArrayList;
import java.util.List;
import net.fabricmc.loader.api.metadata.CustomValue;
import net.fabricmc.loader.api.metadata.ModMetadata;
import net.fabricmc.loader.api.metadata.Person;

/** One line per mod's metadata, the same text on a JVM and in Horizon. */
public final class Describe {
    static String people(java.util.Collection<Person> people) {
        List<String> out = new ArrayList<>();
        for (Person person : people) {
            out.add(person.getName() + "<" + person.getContact().get("email").orElse("-") + ">");
        }
        return String.join(",", out);
    }

    static String cv(CustomValue value) {
        if (value == null) {
            return "absent";
        }
        CustomValue.CvType type = value.getType();
        if (type == CustomValue.CvType.OBJECT) {
            CustomValue.CvObject object = value.getAsObject();
            return "OBJECT(size=" + object.size() + ",a=" + cv(object.get("mixin.a"))
                + ",has_b=" + object.containsKey("mixin.b") + ",badges=" + cv(object.get("badges")) + ")";
        }
        if (type == CustomValue.CvType.ARRAY) {
            CustomValue.CvArray array = value.getAsArray();
            String outOfRange;
            try {
                array.get(5);
                outOfRange = "none";
            } catch (IndexOutOfBoundsException error) {
                outOfRange = error.getMessage();
            }
            return "ARRAY(size=" + array.size() + ",0=" + cv(array.get(0)) + ",5=" + outOfRange + ")";
        }
        if (type == CustomValue.CvType.STRING) {
            return "STRING(" + value.getAsString() + ")";
        }
        if (type == CustomValue.CvType.BOOLEAN) {
            return "BOOLEAN(" + value.getAsBoolean() + ")";
        }
        return type.name() + "#" + type.ordinal();
    }

    public static String line(ModMetadata m) {
        return m.getId() + ": type=" + m.getType() + " version=" + m.getVersion().getFriendlyString()
            + " name=" + m.getName() + " description=[" + m.getDescription() + "]"
            + " environment=" + m.getEnvironment().name() + " authors=" + people(m.getAuthors())
            + " contributors=" + people(m.getContributors())
            + " homepage=" + m.getContact().get("homepage").orElse("-")
            + " issues=" + m.getContact().get("issues").orElse("-")
            + " license=" + m.getLicense() + " provides=" + m.getProvides()
            + " options=" + cv(m.getCustomValue("lithium:options")) + " modmenu=" + cv(m.getCustomValue("modmenu"))
            + " note=" + cv(m.getCustomValue("note")) + " count=" + cv(m.getCustomValue("count"))
            + " nothing=" + cv(m.getCustomValue("nothing")) + " containsNothing=" + m.containsCustomValue("nothing")
            + " containsMissing=" + m.containsCustomValue("missing");
    }
}
