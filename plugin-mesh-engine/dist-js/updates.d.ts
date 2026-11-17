export interface MeshUpdateState {
    hasUpdate: boolean;
    tag?: string;
}
export declare function getMeshUpdateState(): MeshUpdateState;
export declare function onUpdateMeshState(fn: (s: MeshUpdateState) => void): () => void;
export declare function setMeshUpdateState(next: MeshUpdateState): void;
export declare function checkMeshUpdate(): Promise<MeshUpdateState>;
export declare function initMeshUpdateWatcher(): void;
