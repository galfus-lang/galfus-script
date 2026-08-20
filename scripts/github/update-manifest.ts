import { $ } from 'bun';

type UpdateManifestOptions = {
  tag: string;
  version: string;
  component: string;
};

type Manifest = {
  tags: Record<string, string>;
  latest_tag: string;
};

const ORDER_OF_PRECEDENCE = ['stable', 'beta', 'alpha', 'next'];

export async function updateManifest(options: UpdateManifestOptions): Promise<void> {
  const { tag, version, component } = options;

  if (!tag || !version || !component) {
    throw new Error('Missing required arguments: --tag, --version, or --component');
  }

  const storageId = process.env.STORAGE_ID;
  const storageEndpoint = process.env.STORAGE_ENDPOINT;

  if (!storageId || !storageEndpoint) {
    throw new Error('STORAGE_ID and STORAGE_ENDPOINT environment variables are required');
  }

  const s3Path = `s3://${storageId}/manifest.json`;
  const localManifest = `/tmp/manifest.json`;

  console.log(`Downloading existing manifest from ${s3Path}...`);

  let manifest: Manifest = { tags: {}, latest_tag: tag };

  try {
    await $`aws s3 cp ${s3Path} ${localManifest} --endpoint-url ${storageEndpoint} --no-progress`.quiet();
    const content = await Bun.file(localManifest).json();
    if (content && typeof content === 'object') {
      manifest = content as Manifest;
      if (!manifest.tags) manifest.tags = {};
    }
  } catch (err) {
    console.log('No existing manifest found or failed to download. Creating a new one.');
  }

  // Inject new version
  manifest.tags[tag] = version;

  // Resolve latest_tag
  let latestTag = tag;
  for (const t of ORDER_OF_PRECEDENCE) {
    if (manifest.tags[t]) {
      latestTag = t;
      break;
    }
  }

  manifest.latest_tag = latestTag;

  console.log(`Updating manifest.json:`, manifest);

  await Bun.write(localManifest, JSON.stringify(manifest, null, 2));

  console.log(`Uploading updated manifest to ${s3Path}...`);

  await $`aws s3 cp ${localManifest} ${s3Path} --endpoint-url ${storageEndpoint} --no-progress`;

  console.log('Manifest updated successfully.');
}
