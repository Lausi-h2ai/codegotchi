import { useCallback, useEffect, useRef, useState } from "react";

import { CodeGotchiClient } from "./client";
import type { ClientError, ClientStatus } from "./client";
import type { SimulationSnapshot } from "./protocol";

export interface CodeGotchiState {
    snapshot: SimulationSnapshot | null;
    connectionStatus: ClientStatus;
    error: ClientError | null;
    feedback: string | null;
    feed: (foodId: "kibble" | "treat" | "fruit") => Promise<void>;
    clean: (poopId: string) => Promise<void>;
}

declare global {
    interface Window {
        __codeGotchiTestDisconnect?: () => void;
    }
}

export function useCodeGotchi(token: string | null): CodeGotchiState {
    const [snapshot, setSnapshot] = useState<SimulationSnapshot | null>(null);
    const [connectionStatus, setConnectionStatus] =
        useState<ClientStatus>("loading");
    const [error, setError] = useState<ClientError | null>(null);
    const [feedback, setFeedback] = useState<string | null>(null);
    const clientRef = useRef<CodeGotchiClient | null>(null);

    useEffect(() => {
        if (!token) {
            setConnectionStatus("disconnected");
            setError({
                code: "missing_token",
                message:
                    "Launch CodeGotchi with a #token=… link to connect to the pet room.",
            });
            return;
        }

        const client = new CodeGotchiClient(token);
        clientRef.current = client;
        const stop = client.start({
            onSnapshot: (nextSnapshot) => {
                setSnapshot(nextSnapshot);
                setError(null);
            },
            onStatus: (nextStatus) => setConnectionStatus(nextStatus),
            onError: (nextError) => setError(nextError),
        });

        if (import.meta.env.DEV) {
            window.__codeGotchiTestDisconnect = () =>
                client.disconnectForTest();
        }

        return () => {
            stop();
            if (window.__codeGotchiTestDisconnect) {
                delete window.__codeGotchiTestDisconnect;
            }
            clientRef.current = null;
        };
    }, [token]);

    const feed = useCallback(async (foodId: "kibble" | "treat" | "fruit") => {
        const client = clientRef.current;
        if (!client) {
            return;
        }
        try {
            await client.feed(foodId);
            setError(null);
            setFeedback(`Eating ${foodId}`);
        } catch (nextError) {
            setError(asClientError(nextError));
        }
    }, []);

    const clean = useCallback(async (poopId: string) => {
        const client = clientRef.current;
        if (!client) {
            return;
        }
        try {
            await client.clean(poopId);
            setError(null);
            setFeedback("Cleaned up");
        } catch (nextError) {
            setError(asClientError(nextError));
        }
    }, []);

    return { snapshot, connectionStatus, error, feedback, feed, clean };
}

function asClientError(error: unknown): ClientError {
    if (
        typeof error === "object" &&
        error !== null &&
        "code" in error &&
        "message" in error &&
        typeof error.code === "string" &&
        typeof error.message === "string"
    ) {
        return error as ClientError;
    }
    return {
        code: "client_error",
        message: error instanceof Error ? error.message : String(error),
    };
}
