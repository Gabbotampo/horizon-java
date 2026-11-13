package net.fabricmc.loader.api.metadata;
public interface CustomValue {
    CvType getType();
    CvObject getAsObject();
    CvArray getAsArray();
    String getAsString();
    boolean getAsBoolean();
    interface CvObject {
        int size();
        boolean containsKey(String key);
        CustomValue get(String key);
    }
    interface CvArray {
        int size();
        CustomValue get(int index);
    }
    enum CvType { OBJECT, ARRAY, STRING, NUMBER, BOOLEAN, NULL }
}
