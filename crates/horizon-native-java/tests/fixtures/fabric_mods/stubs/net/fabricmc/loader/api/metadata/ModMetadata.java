package net.fabricmc.loader.api.metadata;
import java.util.Collection;
public interface ModMetadata {
    String getType();
    String getId();
    Collection<String> getProvides();
    net.fabricmc.loader.api.Version getVersion();
    ModEnvironment getEnvironment();
    String getName();
    String getDescription();
    Collection<Person> getAuthors();
    Collection<Person> getContributors();
    ContactInformation getContact();
    Collection<String> getLicense();
    boolean containsCustomValue(String key);
    CustomValue getCustomValue(String key);
}
