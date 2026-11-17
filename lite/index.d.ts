export interface Options {
    /** Rewrite a URL before the preview's URL policy; null omits it. */
    link?: (url: string, image: boolean) => string | null;
}

/** Render completed input in the preview subset, without a browser DOM. */
export function render(source: string, options?: Options): string;

export interface Block {
    /** May change when provisional input is reclassified. The application places this node. */
    readonly node: Element;
    /** Once true, this record and its node are never updated again. */
    readonly settled: boolean;
}

export interface StreamOptions extends Options {
    /** Defaults to the browser's document. */
    document?: Document;
}

/** Live block contents. The application owns placement, grouping, scheduling and final replacement. */
export class Stream {
    constructor(options?: StreamOptions);
    readonly source: string;
    readonly blocks: readonly Block[];
    write(chunk: string): readonly Block[];
    update(source: string): readonly Block[];
    end(): readonly Block[];
    reset(): readonly Block[];
}
