import { expect, test, type Page } from "@playwright/test";

const launchUrl = "/#token=task3-playwright-token";
const fixtureToken = "task3-playwright-token";
const motionRegions = new Set([
    "window",
    "shelf",
    "floor-left",
    "floor-center",
    "floor-right",
    "furniture",
]);

type FixtureState = {
    needs: {
        hunger: number;
        energy: number;
        happiness: number;
        cleanliness: number;
    };
    pendingDemands: Array<{ id: string; kind: string }>;
    pendingPoops: Array<{ id: string }>;
    attentionSequence: number;
    lastUpdatedAt: string;
    nextIncidentAt: string;
};

let nextHookEventId = 0xaa01;

type HookEventOptions = {
    kind:
        | "session_started"
        | "session_ended"
        | "turn_started"
        | "turn_completed"
        | "waiting_for_user"
        | "output_activity"
        | "tool_started"
        | "tool_completed"
        | "command_started"
        | "command_completed"
        | "interrupted"
        | "integration_error";
    activity?:
        | "idle"
        | "thinking"
        | "reading"
        | "searching"
        | "editing"
        | "testing"
        | "building"
        | "installing"
        | "git_operation"
        | "docker_operation"
        | "web_research"
        | "waiting"
        | "celebrating"
        | "error"
        | "blocked"
        | "unknown_work"
        | null;
    exitStatus?: number | null;
    permission?: {
        category: "development";
        purpose: "safe_development";
    };
};

function nextEventId(): string {
    const suffix = (nextHookEventId++).toString(16).padStart(12, "0");
    return `00000000-0000-0000-0000-${suffix}`;
}

async function sendHookEvent(
    page: Page,
    options: HookEventOptions,
): Promise<Record<string, unknown>> {
    const response = await page.evaluate(
        async ({ eventId, kind, activity, exitStatus, permission }) => {
            const result = await fetch("/api/v1/events", {
                method: "POST",
                headers: {
                    Authorization: "Bearer task3-playwright-token",
                    "Content-Type": "application/json",
                },
                body: JSON.stringify({
                    event: {
                        id: eventId,
                        schema_version: 1,
                        session_id: "00000000-0000-0000-0000-000000000007",
                        repository_id: "playwright-fixture",
                        source: "codex",
                        kind,
                        activity: activity ?? null,
                        timestamp: new Date().toISOString(),
                        metadata: {
                            executable_name: null,
                            command_category: null,
                            exit_status: exitStatus ?? null,
                            duration_ms: null,
                            blocked: false,
                        },
                    },
                    permission: permission ?? undefined,
                }),
            });
            return { status: result.status, body: await result.json() };
        },
        {
            eventId: nextEventId(),
            ...options,
        },
    );
    expect(response.status).toBe(200);
    expect(response.body.accepted).toBe(true);
    return response.body as Record<string, unknown>;
}

async function resetFixture(page: Page, mode: string): Promise<void> {
    const response = await page.request.post(
        `/__fixture/reset?mode=${encodeURIComponent(mode)}`,
    );
    expect(response.status()).toBe(200);
    const body = (await response.json()) as { mode?: string };
    expect(body.mode).toBe(mode);
}

async function fetchAuthoritativeState(page: Page): Promise<FixtureState> {
    const response = await page.evaluate(async (token) => {
        const result = await fetch("/api/v1/state", {
            headers: { Authorization: `Bearer ${token}` },
        });
        return { status: result.status, body: await result.json() };
    }, fixtureToken);
    expect(response.status).toBe(200);
    return response.body as FixtureState;
}

async function needValue(page: Page, label: string): Promise<number> {
    const text = await page
        .locator(".need strong")
        .filter({ hasText: label })
        .textContent();
    const value = Number.parseFloat(
        text?.match(/([0-9]+(?:\.[0-9]+)?)%$/)?.[1] ?? "NaN",
    );
    expect(value).toBeGreaterThanOrEqual(0);
    return value;
}

function petLocator(page: Page) {
    return page.locator('[data-testid="pet"]');
}

async function motionAttribute(
    page: Page,
    attribute: string,
): Promise<string | null> {
    return petLocator(page).getAttribute(attribute);
}

test.describe.serial("CodeGotchi production browser vertical slice", () => {
    test("loads embedded production bytes and follows authoritative activity", async ({
        page,
    }) => {
        let initialStateAborted = false;
        await page.route(/\/api\/v1\/state(?:\?|$)/, async (route) => {
            if (!initialStateAborted) {
                initialStateAborted = true;
                await route.abort("failed");
                return;
            }
            await route.continue();
        });

        await page.goto(launchUrl);

        await expect.poll(() => initialStateAborted).toBe(true);
        await expect(page.locator('script[src^="/src/"]')).toHaveCount(0);
        await expect(page.locator('script[src^="/@vite"]')).toHaveCount(0);
        await expect(
            page.getByRole("region", { name: "CodeGotchi pet room" }),
        ).toBeVisible();
        await expect(page.getByRole("heading", { level: 2 })).toContainText(
            "Mochi",
        );
        await expect(page.getByText("Connected")).toBeVisible();
        await expect(page.getByRole("alert")).toHaveCount(0);
        await expect(
            page.getByRole("region", { name: "Desk and work area" }),
        ).toBeVisible();
        await expect(page.getByTestId("food-treat")).toContainText("25");
        await expect(page.getByTestId(/poop-/).first()).toBeVisible();
        await expect(page.getByTestId("activity-label")).toContainText(
            "Working",
        );
        await expect(page).not.toHaveURL(/#token=/);

        const eventResponse = await page.evaluate(
            async (request) => {
                const response = await fetch("/api/v1/events", {
                    method: "POST",
                    headers: {
                        Authorization: "Bearer task3-playwright-token",
                        "Content-Type": "application/json",
                    },
                    body: JSON.stringify(request),
                });
                return { status: response.status, body: await response.json() };
            },
            {
                event: {
                    id: "00000000-0000-0000-0000-00000000f001",
                    schema_version: 1,
                    session_id: "00000000-0000-0000-0000-000000000007",
                    repository_id: "playwright-fixture",
                    source: "codex",
                    kind: "turn_started",
                    activity: null,
                    timestamp: new Date().toISOString(),
                    metadata: {
                        executable_name: null,
                        command_category: null,
                        exit_status: null,
                        duration_ms: null,
                        blocked: false,
                    },
                },
            },
        );
        expect(eventResponse.status).toBe(200);
        await expect(page.getByTestId("activity-label")).toContainText(
            "Thinking",
        );
    });

    test("feeds with drag and drop and keeps the authoritative result after reload", async ({
        page,
    }) => {
        await page.goto(launchUrl);
        const food = page.getByTestId("food-treat");
        const feedTarget = page.getByRole("button", {
            name: /feed target/i,
        });
        const before = await food.locator("strong").textContent();

        await food.dragTo(feedTarget);
        await expect(page.getByText("Eating a treat")).toBeVisible();
        await expect(food.locator("strong")).toHaveText(
            String(Number(before) - 1),
        );

        await page.reload();
        await expect(food.locator("strong")).toHaveText(
            String(Number(before) - 1),
        );
    });

    test("drinks an energy drink and keeps the authoritative count after reload", async ({
        page,
    }) => {
        await page.goto(launchUrl);
        const drink = page.getByTestId("food-energy_drink");
        const feedTarget = page.getByRole("button", {
            name: /feed target/i,
        });
        const before = await drink.locator("strong").textContent();

        await drink.dragTo(feedTarget);
        await expect(page.getByText("Eating an energy drink")).toBeVisible();
        await expect(drink.locator("strong")).toHaveText(
            String(Number(before) - 1),
        );

        await page.reload();
        await expect(drink.locator("strong")).toHaveText(
            String(Number(before) - 1),
        );
    });

    test("takes a five-second hammock nap, then wakes up", async ({ page }) => {
        await page.goto(launchUrl);
        const hammock = page.getByRole("button", { name: "Hammock nap" });

        await hammock.click();

        await expect(
            page.getByRole("button", { name: "Resting in hammock" }),
        ).toBeVisible();
        await expect(page.getByTestId("zzz")).toBeVisible();
        await expect(page.getByTestId("activity-label")).toContainText(
            "Sleeping",
        );
        await expect(page.getByText("Cozy nap in the hammock…")).toBeVisible();

        await expect(
            page.getByRole("button", { name: "Hammock nap" }),
        ).toBeVisible({ timeout: 8_000 });
        await expect(page.getByTestId("zzz")).toHaveCount(0);
    });

    test("does not send an invalid food drop", async ({ page }) => {
        await page.goto(launchUrl);
        const food = page.getByTestId("food-treat");
        const before = await food.locator("strong").textContent();

        await food.dragTo(page.getByRole("button", { name: "Trash" }));
        await page.waitForTimeout(150);

        await expect(food.locator("strong")).toHaveText(before ?? "");
        await expect(page.getByRole("alert")).toHaveCount(0);
    });

    test("does not clean a poop dragged directly to trash", async ({
        page,
    }) => {
        await page.goto(launchUrl);
        const poops = page.locator("[data-poop-id]");
        const before = await poops.count();

        await poops.first().dragTo(page.getByRole("button", { name: "Trash" }));
        await page.waitForTimeout(150);

        await expect(poops).toHaveCount(before);
        await expect(page.getByText("Cleaned up")).toHaveCount(0);
    });

    test("presents a real backend care error", async ({ page }) => {
        await page.goto(launchUrl);
        const emptyFood = page.getByTestId("food-kibble");

        await emptyFood.dragTo(
            page.getByRole("button", { name: /feed target/i }),
        );

        await expect(page.getByRole("alert")).toContainText("out of stock");
    });

    test("cleans a poop only after shovel, poop, and trash, then persists removal", async ({
        page,
    }) => {
        await page.goto(launchUrl);
        const poops = page.locator("[data-poop-id]");
        const before = await poops.count();

        await page.getByRole("button", { name: "Shovel" }).click();
        await poops.first().click();
        await expect(page.getByRole("button", { name: "Trash" })).toBeVisible();
        await page.getByRole("button", { name: "Trash" }).click();

        await expect(poops).toHaveCount(before - 1);
        await expect(page.getByText("Cleaned up")).toBeVisible();

        await page.reload();
        await expect(page.locator("[data-poop-id]")).toHaveCount(before - 1);
    });

    test("recovers from a disconnected stream and accepts the replacement snapshot", async ({
        page,
    }) => {
        let connectionCount = 0;
        let disconnect: (() => Promise<void>) | undefined;

        await page.routeWebSocket(/\/api\/v1\/stream(?:\?|$)/, (webSocket) => {
            connectionCount += 1;
            const server = webSocket.connectToServer();
            disconnect = () => webSocket.close();

            webSocket.onMessage((message) => server.send(message));
            server.onMessage((message) => webSocket.send(message));
        });

        await page.goto(launchUrl);
        await expect(page.getByText("Connected")).toBeVisible();
        await expect.poll(() => connectionCount).toBe(1);
        expect(disconnect).toBeDefined();

        await disconnect?.();

        await expect(page.getByText("Reconnecting…")).toBeVisible({
            timeout: 5_000,
        });
        await expect.poll(() => connectionCount).toBe(2);
        await expect(page.getByText("Connected")).toBeVisible();
        await expect(page.getByRole("heading", { level: 2 })).toContainText(
            "Mochi",
        );
        await expect(page.getByTestId("activity-label")).toContainText(
            "Thinking",
        );
    });

    test("keeps idle travel in safe regions and eventually performs a roll", async ({
        page,
    }) => {
        test.setTimeout(40_000);
        await page.goto(launchUrl);
        await expect(petLocator(page)).toBeVisible();

        await sendHookEvent(page, {
            kind: "waiting_for_user",
            activity: "waiting",
        });
        await expect(page.getByTestId("activity-label")).toContainText(
            /Idle|Waiting/,
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-mode",
            "free_time",
        );

        const observedRegions = new Set<string>();
        let observedRoll = false;
        let firstUngroundedPosition:
            | {
                  region: string;
                  floorTop: number;
                  roomBottom: number;
                  petFeet: number;
              }
            | undefined;
        const pollOptions = {
            timeout: 35_000,
            intervals: [100, 250, 500],
        } as const;
        const isIdle = (label: string | null): boolean =>
            label?.match(/Idle|Waiting/) !== null;
        const renderedRoomGeometry = async () =>
            page.evaluate(() => {
                const room =
                    document.querySelector<HTMLElement>(".room-illustration");
                const pet = document.querySelector<HTMLElement>(
                    '[data-testid="pet"]',
                );
                if (!room || !pet) {
                    return null;
                }
                const roomRect = room.getBoundingClientRect();
                const petRect = pet.getBoundingClientRect();
                return {
                    floorTop: roomRect.top + roomRect.height * 0.7,
                    roomBottom: roomRect.bottom,
                    petFeet: petRect.bottom,
                };
            });

        await Promise.all([
            expect
                .poll(
                    async () => {
                        const [region, mode, phase, label] = await Promise.all([
                            motionAttribute(page, "data-motion-region"),
                            motionAttribute(page, "data-motion-mode"),
                            motionAttribute(page, "data-motion-phase"),
                            page.getByTestId("activity-label").textContent(),
                        ]);
                        if (mode === "free_time" && region) {
                            observedRegions.add(region);
                            if (phase !== "traveling") {
                                const geometry = await renderedRoomGeometry();
                                if (
                                    !firstUngroundedPosition &&
                                    geometry !== null &&
                                    (geometry.petFeet < geometry.floorTop - 8 ||
                                        geometry.petFeet >
                                            geometry.roomBottom + 1)
                                ) {
                                    firstUngroundedPosition = {
                                        region,
                                        ...geometry,
                                    };
                                }
                            }
                        }
                        return observedRegions.size >= 2 && isIdle(label);
                    },
                    {
                        ...pollOptions,
                        message:
                            "free-time choreography should visit multiple safe regions",
                    },
                )
                .toBe(true),
            expect
                .poll(
                    async () => {
                        const [action, mode, label] = await Promise.all([
                            motionAttribute(page, "data-motion-action"),
                            motionAttribute(page, "data-motion-mode"),
                            page.getByTestId("activity-label").textContent(),
                        ]);
                        observedRoll ||=
                            mode === "free_time" && action === "roll";
                        return observedRoll && isIdle(label);
                    },
                    {
                        ...pollOptions,
                        message:
                            "free-time choreography should perform a roll within its 15–30s interval",
                    },
                )
                .toBe(true),
        ]);

        expect(
            [...observedRegions].every((region) => motionRegions.has(region)),
        ).toBe(true);
        expect(observedRegions.size).toBeGreaterThanOrEqual(2);
        expect(firstUngroundedPosition).toBeUndefined();
        await expect(page.getByTestId("activity-label")).toContainText(
            /Idle|Waiting/,
        );
    });

    test("maps searching and editing events to interruptible thinking and desk poses", async ({
        page,
    }) => {
        await page.goto(launchUrl);
        await expect(petLocator(page)).toBeVisible();
        await sendHookEvent(page, {
            kind: "waiting_for_user",
            activity: "waiting",
        });
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-mode",
            "free_time",
        );
        await expect
            .poll(() => motionAttribute(page, "data-motion-phase"), {
                timeout: 11_000,
                intervals: [50],
            })
            .toBe("traveling");
        const idleEyeTop = await petLocator(page)
            .locator(".pet-eye")
            .first()
            .evaluate((eye) => Number.parseFloat(getComputedStyle(eye).top));

        await sendHookEvent(page, {
            kind: "turn_started",
            activity: "searching",
        });
        const thinkingStarted = Date.now();
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-mode",
            "thinking",
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-waypoint",
            "thinking",
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-action",
            "think",
            { timeout: 999 },
        );
        expect(Date.now() - thinkingStarted).toBeLessThan(1_000);
        await expect(page.locator(".thought-bubbles")).toBeVisible();
        await expect(page.getByTestId("activity-label")).toContainText(
            "Searching",
        );
        const thinkingEyeTop = await petLocator(page)
            .locator(".pet-eye")
            .first()
            .evaluate((eye) => Number.parseFloat(getComputedStyle(eye).top));
        expect(thinkingEyeTop).toBeLessThan(idleEyeTop);

        const idleMonitorPresentation = await page
            .locator(".monitor")
            .evaluate((monitor) => {
                const style = getComputedStyle(monitor);
                const screen = monitor.querySelector("span");
                const screenStyle = screen ? getComputedStyle(screen) : null;
                return [
                    style.backgroundColor,
                    style.boxShadow,
                    style.filter,
                    style.animationName,
                    screenStyle?.backgroundColor ?? "",
                    screenStyle?.boxShadow ?? "",
                    screenStyle?.animationName ?? "",
                ];
            });

        await sendHookEvent(page, {
            kind: "command_started",
            activity: "editing",
        });
        const deskStarted = Date.now();
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-mode",
            "desk",
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-waypoint",
            "desk",
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-facing",
            "right",
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-action",
            "type",
            { timeout: 999 },
        );
        expect(Date.now() - deskStarted).toBeLessThan(1_000);
        await expect(page.locator(".typing-marks")).toBeVisible();
        await expect(page.getByTestId("activity-label")).toContainText(
            "Typing / editing",
        );
        const typingMarkGeometry = await page
            .getByTestId("typing-marks")
            .locator("span")
            .evaluateAll((marks) =>
                marks.map((mark) => {
                    const style = getComputedStyle(mark);
                    return {
                        width: Number.parseFloat(style.width),
                        height: Number.parseFloat(style.height),
                    };
                }),
            );
        expect(typingMarkGeometry).toHaveLength(3);
        expect(
            typingMarkGeometry.every(
                ({ width, height }) => width > 0 && height > 0,
            ),
        ).toBe(true);
        await expect
            .poll(
                () =>
                    page.locator(".monitor").evaluate((monitor) => {
                        const style = getComputedStyle(monitor);
                        const screen = monitor.querySelector("span");
                        const screenStyle = screen
                            ? getComputedStyle(screen)
                            : null;
                        return [
                            style.backgroundColor,
                            style.boxShadow,
                            style.filter,
                            style.animationName,
                            screenStyle?.backgroundColor ?? "",
                            screenStyle?.boxShadow ?? "",
                            screenStyle?.animationName ?? "",
                        ];
                    }),
                { timeout: 1_000 },
            )
            .not.toEqual(idleMonitorPresentation);

        await sendHookEvent(page, {
            kind: "turn_started",
            activity: "thinking",
        });
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-mode",
            "thinking",
            { timeout: 1_000 },
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-waypoint",
            "thinking",
            { timeout: 1_000 },
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-action",
            "think",
            { timeout: 999 },
        );
    });

    test("keeps semantic poses static when reduced motion is requested", async ({
        page,
    }) => {
        await page.emulateMedia({ reducedMotion: "reduce" });
        await page.goto(launchUrl);
        await expect(petLocator(page)).toBeVisible();

        await sendHookEvent(page, {
            kind: "command_started",
            activity: "testing",
        });
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-mode",
            "desk",
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-waypoint",
            "desk",
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-phase",
            "static",
        );
        await expect
            .poll(() => motionAttribute(page, "data-motion-action"))
            .not.toBe("roll");
        await expect(page.locator(".thought-bubbles")).toHaveCount(0);

        await sendHookEvent(page, {
            kind: "turn_started",
            activity: "searching",
        });
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-mode",
            "thinking",
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-waypoint",
            "thinking",
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-phase",
            "static",
        );
        await expect
            .poll(() => motionAttribute(page, "data-motion-action"))
            .not.toBe("roll");
        await expect
            .poll(() => motionAttribute(page, "data-motion-action"))
            .not.toBe("think");
        await expect(page.locator(".thought-bubbles")).toHaveCount(0);
    });

    test("keeps care controls authoritative while free-time motion is traveling", async ({
        page,
    }) => {
        await page.goto(launchUrl);
        await expect(petLocator(page)).toBeVisible();
        await sendHookEvent(page, {
            kind: "waiting_for_user",
            activity: "waiting",
        });
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-mode",
            "free_time",
        );
        await expect(petLocator(page)).toHaveAttribute(
            "data-motion-phase",
            "traveling",
        );

        const fruit = page.getByTestId("food-fruit");
        const beforeFruit = Number(await fruit.locator("strong").textContent());
        await fruit.click();
        await expect(page.getByText("Eating fruit")).toBeVisible();
        await expect(fruit.locator("strong")).toHaveText(
            String(beforeFruit - 1),
        );

        const restock = page.getByTestId("restock");
        await expect(restock).toBeVisible();
        await restock.click();
        await expect(page.getByText("Restocked the pantry")).toBeVisible();
        await expect(fruit.locator("strong")).toHaveText("25");

        const poops = page.locator("[data-poop-id]");
        const beforePoops = await poops.count();
        expect(beforePoops).toBeGreaterThan(0);
        await page.getByRole("button", { name: "Shovel" }).click();
        await poops.first().click();
        await page.getByRole("button", { name: "Trash" }).click();
        await expect(page.getByText("Cleaned up")).toBeVisible();
        await expect(poops).toHaveCount(beforePoops - 1);

        await page.getByRole("button", { name: "Hammock nap" }).click();
        await expect(
            page.getByRole("button", { name: "Resting in hammock" }),
        ).toBeVisible();
        await expect(page.getByTestId("activity-label")).toContainText(
            "Sleeping",
        );
        await expect(
            page.getByRole("button", { name: "Hammock nap" }),
        ).toBeVisible({ timeout: 8_000 });
    });

    test("resolves an affection demand with a real sustained petting gesture", async ({
        page,
    }) => {
        await resetFixture(page, "affection");
        await page.goto(launchUrl);

        await expect(page.getByText("Needs attention")).toBeVisible();
        await expect(page.getByTestId("demand-affection-count")).toHaveText(
            "1",
        );
        const happinessBefore = await needValue(page, "Happiness");
        const pet = petLocator(page);
        await pet.evaluate((element) =>
            element.scrollIntoView({ block: "center", inline: "center" }),
        );
        const box = await pet.boundingBox();
        expect(box).not.toBeNull();
        if (!box) {
            return;
        }

        const startX = box.x + box.width / 2;
        const startY = box.y + box.height / 2;
        await page.mouse.move(startX, startY);
        await page.mouse.down();
        await page.waitForTimeout(800);
        await page.mouse.move(startX + 70, startY, { steps: 20 });
        await page.waitForTimeout(800);
        await page.mouse.move(startX + 70, startY + 70, { steps: 20 });
        await page.mouse.up();

        await expect(page.getByText("Needs attention")).toHaveCount(0, {
            timeout: 5_000,
        });
        expect(await needValue(page, "Happiness")).toBeGreaterThan(
            happinessBefore,
        );
        const state = await fetchAuthoritativeState(page);
        expect(
            state.pendingDemands.some((demand) => demand.kind === "affection"),
        ).toBe(false);
    });

    test("resolves one snack demand and one poop through the existing care UI", async ({
        page,
    }) => {
        await resetFixture(page, "snack-poop");
        await page.goto(launchUrl);

        await expect(page.getByText("Wants a snack")).toBeVisible();
        await expect(page.getByTestId("demand-snack-count")).toHaveText("1");
        const poops = page.locator("[data-poop-id]");
        await expect(poops).toHaveCount(1);

        await page
            .getByTestId("food-treat")
            .dragTo(page.getByRole("button", { name: /feed target/i }));
        await expect(page.getByText("Eating a treat")).toBeVisible();
        await expect(page.getByText("Wants a snack")).toHaveCount(0);
        await expect(page.getByTestId("demand-snack-count")).toHaveCount(0);

        await page.getByRole("button", { name: "Shovel" }).click();
        await poops.first().click();
        await expect(page.getByRole("button", { name: "Trash" })).toBeVisible();
        await page.getByRole("button", { name: "Trash" }).click();
        await expect(page.getByText("Cleaned up")).toBeVisible();
        await expect(poops).toHaveCount(0);

        const state = await fetchAuthoritativeState(page);
        expect(
            state.pendingDemands.some((demand) => demand.kind === "snack"),
        ).toBe(false);
        expect(state.pendingPoops).toHaveLength(0);
    });

    test("denies a safe development PreToolUse fixture for severe happiness neglect", async ({
        page,
    }) => {
        await resetFixture(page, "strict-happiness");
        await page.goto(launchUrl);
        await expect(page.getByText("Connected")).toBeVisible();

        const state = await fetchAuthoritativeState(page);
        expect(state.needs.happiness).toBeLessThanOrEqual(5);
        expect(state.needs.hunger).toBeLessThan(70);
        expect(state.needs.energy).toBeGreaterThan(30);
        expect(state.needs.cleanliness).toBeGreaterThan(30);

        const response = await sendHookEvent(page, {
            kind: "tool_started",
            activity: "testing",
            permission: {
                category: "development",
                purpose: "safe_development",
            },
        });
        expect(response.blocked).toBe(true);
        expect(response.reason).toContain("desperately needs attention");
        expect(response.reason).toContain("Pet it in the CodeGotchi UI");
    });

    test("catches up overdue persisted care pressure once and keeps it after refresh", async ({
        page,
    }) => {
        await resetFixture(page, "overdue");
        await page.goto(launchUrl);

        const before = await fetchAuthoritativeState(page);
        expect(Date.parse(before.lastUpdatedAt)).toBeLessThan(Date.now());
        expect(before.pendingDemands).toHaveLength(0);
        expect(before.pendingPoops).toHaveLength(0);

        let after = before;
        await expect
            .poll(
                async () => {
                    after = await fetchAuthoritativeState(page);
                    return (
                        after.pendingDemands.length + after.pendingPoops.length
                    );
                },
                { timeout: 10_000, intervals: [100, 250, 500] },
            )
            .toBeGreaterThan(0);

        const caughtUpCount =
            after.pendingDemands.length + after.pendingPoops.length;
        expect(caughtUpCount).toBeGreaterThanOrEqual(1);
        expect(caughtUpCount).toBeLessThanOrEqual(5);
        expect(after.needs.hunger).toBeGreaterThan(before.needs.hunger);
        expect(after.needs.energy).toBeLessThan(before.needs.energy);
        expect(after.needs.happiness).toBeLessThan(before.needs.happiness);
        expect(after.needs.cleanliness).toBeLessThan(before.needs.cleanliness);
        expect(Date.parse(after.nextIncidentAt)).toBeGreaterThan(Date.now());

        const demandIds = after.pendingDemands.map((demand) => demand.id);
        const poopIds = after.pendingPoops.map((poop) => poop.id);
        await page.reload();
        await expect(page.getByText("Connected")).toBeVisible();
        const refreshed = await fetchAuthoritativeState(page);
        expect(refreshed.pendingDemands.map((demand) => demand.id)).toEqual(
            demandIds,
        );
        expect(refreshed.pendingPoops.map((poop) => poop.id)).toEqual(poopIds);
        expect(refreshed.attentionSequence).toBe(after.attentionSequence);
    });
});
