import { API } from "../src/api/async/api.ts";

const [projectFile, sourceFile] = process.argv.slice(2);
if (!projectFile || !sourceFile) {
    process.stderr.write("usage: npm run example:semantic-snapshot -- <tsconfig.json> <source-file>\n");
    process.exitCode = 2;
}
else {
    const api = new API({ cwd: process.env.npm_config_local_prefix ?? process.env.INIT_CWD ?? process.cwd() });
    try {
        const snapshot = await api.updateSnapshot({ openProject: projectFile });
        const project = snapshot.getProject(projectFile);
        if (!project) {
            throw new Error(`Project was not loaded: ${projectFile}`);
        }
        const result = await project.getSemanticSnapshot({
            schemaVersion: 1,
            requiredCapabilities: ["occurrence.file-wide", "types.core-composite"],
            files: [sourceFile],
        });
        process.stdout.write(`${JSON.stringify(result, undefined, 2)}\n`);
    }
    finally {
        await api.close();
    }
}
