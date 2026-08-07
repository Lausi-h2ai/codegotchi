import { expect, test } from "@playwright/test";

const launchUrl = "/#token=task3-playwright-token";

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
});
