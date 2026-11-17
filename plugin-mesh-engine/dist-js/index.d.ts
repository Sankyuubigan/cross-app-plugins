export interface MeshQuantStatus {
    quant: string;
    dir: string;
    ggufCount: number;
    pixal3dFiles: number;
}
export interface MeshEngineStatus {
    installed: boolean;
    exePath: string;
    dir: string;
    weights: MeshQuantStatus[];
}
export interface MeshGenerateArgs {
    imagePath: string;
    sessionId: string;
    model?: string;
    quant?: string;
    backend?: string;
    gpu?: number;
    seed?: number;
    resolution?: number;
    bgRemoval?: string;
    faces?: string;
}
export interface MeshGenerateOut {
    glbPath: string;
    port: number;
    durationSecs: number;
}
export declare function getMeshEngineStatus(): Promise<MeshEngineStatus>;
export declare function meshGenerate(args: MeshGenerateArgs): Promise<MeshGenerateOut>;
export interface MeshInstallArgs {
    backend?: string;
    quant?: string;
}
export declare function installMeshEngine(args: MeshInstallArgs): Promise<MeshEngineStatus>;
export declare function setMeshEngineDir(path: string): Promise<MeshEngineStatus>;
export declare function checkMeshEngineUpdate(): Promise<string | null>;
export interface RigEngineStatus {
    ready: boolean;
    exePresent: boolean;
    dllsPresent: boolean;
    weightsPresent: number;
    weightsTotal: number;
    dir: string;
    weightsDir: string;
}
export declare function getRigEngineStatus(): Promise<RigEngineStatus>;
export declare function installRigEngine(): Promise<RigEngineStatus>;
export declare function setRigEngineDir(path: string): Promise<RigEngineStatus>;
export * from "./updates";
import "./web-components";
