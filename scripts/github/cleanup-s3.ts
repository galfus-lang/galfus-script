import { $ } from 'bun';

export type CleanupS3Options = {
  tag: string;
};

export async function cleanupS3(options: CleanupS3Options): Promise<void> {
  const { tag } = options;
  if (!tag) {
    throw new Error('No tag provided for cleanup');
  }

  const storageId = process.env.STORAGE_ID;
  const endpoint = process.env.STORAGE_ENDPOINT;

  if (!storageId || !endpoint) {
    throw new Error('STORAGE_ID and STORAGE_ENDPOINT must be set');
  }

  console.log(`Starting S3 cleanup for tag: ${tag}`);

  // The components we build
  const components = ['cli', 'host-native', 'host-web', 'playground-web'];

  for (const component of components) {
    const basePath = `s3://${storageId}/${component}/${tag}/`;
    console.log(`\nScanning ${basePath}`);

    // List all versions in this component/tag
    let output = '';
    try {
      output = await $`aws s3 ls ${basePath} --endpoint-url ${endpoint}`.text();
    } catch (e) {
      console.log(`Failed to list or no versions found for ${component}/${tag}. Skipping.`);
      continue;
    }

    const lines = output.split('\n').map((l) => l.trim()).filter((l) => l.length > 0);
    const versions: string[] = [];

    for (const line of lines) {
      // aws s3 ls directory output format: "                           PRE 1.0.0/"
      if (line.includes('PRE ')) {
        const parts = line.split('PRE ');
        if (parts.length > 1) {
          const dir = parts[1]!.replace('/', '').trim();
          versions.push(dir);
        }
      }
    }

    if (versions.length === 0) {
      console.log(`No versions found in ${basePath}`);
      continue;
    }

    // Group by major version
    const grouped = new Map<string, string[]>();
    for (const v of versions) {
      const match = v.match(/^(\d+)/);
      if (match) {
        const major = match[1]!;
        if (!grouped.has(major)) {
          grouped.set(major, []);
        }
        grouped.get(major)!.push(v);
      } else {
        // If it doesn't match semantic versioning properly, just skip logic or group under "unknown"
        console.warn(`Version ${v} doesn't start with a number. Skipping.`);
      }
    }

    // Sort major versions descending (numerically)
    const sortedMajors = Array.from(grouped.keys()).sort((a, b) => parseInt(b, 10) - parseInt(a, 10));

    const majorsToKeep = sortedMajors.slice(0, 5);
    const majorsToDelete = sortedMajors.slice(5);

    for (const major of majorsToDelete) {
      const toDelete = grouped.get(major)!;
      for (const v of toDelete) {
        console.log(`[DELETE] Major version too old: ${v}`);
        const pathToDelete = `${basePath}${v}/`;
        await $`aws s3 rm ${pathToDelete} --recursive --endpoint-url ${endpoint}`;
      }
    }

    // For the kept majors, keep only the top 5 versions
    for (const major of majorsToKeep) {
      const vList = grouped.get(major)!;
      // Sort semantic versions descending
      // We assume format X.Y.Z
      vList.sort((a, b) => {
        const pa = a.split('.').map(Number);
        const pb = b.split('.').map(Number);
        for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
          const numA = pa[i] || 0;
          const numB = pb[i] || 0;
          if (numA !== numB) {
            return numB - numA;
          }
        }
        return 0;
      });

      const vToKeep = vList.slice(0, 5);
      const vToDelete = vList.slice(5);

      for (const v of vToDelete) {
        console.log(`[DELETE] Exceeded 5 versions for major ${major}: ${v}`);
        const pathToDelete = `${basePath}${v}/`;
        await $`aws s3 rm ${pathToDelete} --recursive --endpoint-url ${endpoint}`;
      }

      for (const v of vToKeep) {
        console.log(`[KEEP] Major ${major} -> ${v}`);
      }
    }
  }

  console.log('\nCleanup completed.');
}
