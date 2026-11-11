package demo;

import net.neoforged.neoforge.attachment.AttachmentSyncHandler;

/** A sync handler: the bridge records it and counts every sync it would send. */
public final class Sync implements AttachmentSyncHandler<Data> {}
