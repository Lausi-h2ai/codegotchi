import { expect, test } from "@playwright/test";

const launchUrl = "/#token=task3-playwright-token";

test.describe.serial("CodeGotchi Task 3 browser vertical slice", () => {
    test("loads the functional room from a real Task 2 server", async ({
        page,
    }) => {
        await page.goto(launchUrl);

        await expect(
            page.getByRole("region", { name: "CodeGotchi pet room" }),
        ).toBeVisible();
        await expect(page.getByRole("heading", { level: 2 })).toContainText(
            "Mochi",
        );
        await expect(page.getByText("Connected")).toBeVisible();
        await expect(
            page.getByRole("region", { name: "Desk and work area" }),
        ).toBeVisible();
        await expect(page.getByTestId("food-treat")).toContainText("25");
        await expect(page.getByTestId(/poop-/).first()).toBeVisible();
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
        await expect(page.getByText("Eating treat")).toBeVisible();
        await expect(food.locator("strong")).toHaveText(
            String(Number(before) - 1),
        );

        await page.reload();
        await expect(food.locator("strong")).toHaveText(
            String(Number(before) - 1),
        );
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
        await page.goto(launchUrl);
        await expect(page.getByText("Connected")).toBeVisible();

        await page.evaluate(() => {
            const testWindow = window as Window & {
                __codeGotchiTestDisconnect?: () => void;
            };
            testWindow.__codeGotchiTestDisconnect?.();
        });

        await expect(page.getByText("Reconnecting…")).toBeVisible();
        await expect(page.getByText("Connected")).toBeVisible();
        await expect(page.getByRole("heading", { level: 2 })).toContainText(
            "Mochi",
        );
    });
});
