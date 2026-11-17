export declare class MeshEnginePanel extends HTMLElement {
    private root;
    private unlisten?;
    private unsubUpdate?;
    connectedCallback(): void;
    disconnectedCallback(): void;
    private setStatus;
    private setProgress;
    private showProgress;
    private weightsSummary;
    private renderHint;
    private rigSetProgress;
    private showRigProgress;
    private renderRig;
    private refreshRig;
    private onRigInstall;
    private onRigSetDir;
    private refresh;
    private renderUpdateState;
    private onCheckUpdate;
    private onInstall;
    private onSetDir;
}
