import { test, expect } from '@playwright/test';

test.describe('iExtend.html', () => {
  test('loads without JS errors', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (e) => errors.push(e.message));
    page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
    await page.goto('/iExtend.html');
    await page.waitForLoadState('networkidle');
    expect(errors, `console/page errors: ${errors.join(' | ')}`).toEqual([]);
  });

  test('renders 15 artboards across 4 sections', async ({ page }) => {
    await page.goto('/iExtend.html');
    await page.waitForSelector('[data-dc-section]', { timeout: 10_000 });
    const sections = await page.locator('[data-dc-section]').count();
    expect(sections).toBe(4);
    const artboards = await page.locator('[data-dc-slot]').count();
    expect(artboards).toBe(15);
  });

  test('canvas pans on background drag', async ({ page }) => {
    await page.goto('/iExtend.html');
    await page.waitForSelector('[data-dc-section]');
    const world = page.locator('.design-canvas > div').first();
    const before = await world.evaluate((el) => (el as HTMLElement).style.transform);
    // drag from a known empty canvas spot (bottom-left, away from any artboard)
    await page.mouse.move(40, 850);
    await page.mouse.down();
    await page.mouse.move(140, 850, { steps: 5 });
    await page.mouse.up();
    const after = await world.evaluate((el) => (el as HTMLElement).style.transform);
    expect(after).not.toBe(before);
    expect(after).toMatch(/translate3d/);
  });

  test('canvas zooms on ctrl+wheel', async ({ page }) => {
    await page.goto('/iExtend.html');
    await page.waitForSelector('[data-dc-section]');
    const world = page.locator('.design-canvas > div').first();
    const before = await world.evaluate((el) => (el as HTMLElement).style.transform);
    await page.mouse.move(720, 450);
    await page.keyboard.down('Control');
    await page.mouse.wheel(0, -120);
    await page.keyboard.up('Control');
    await page.waitForTimeout(150);
    const after = await world.evaluate((el) => (el as HTMLElement).style.transform);
    expect(after).not.toBe(before);
    expect(after).toMatch(/scale\(/);
  });

  test('artboard labels are visible', async ({ page }) => {
    await page.goto('/iExtend.html');
    await page.waitForSelector('[data-dc-slot]');
    await expect(page.getByText('iPad · Welcome').first()).toBeVisible();
    await expect(page.getByText('iPad · Live (extended)').first()).toBeVisible();
    await expect(page.getByText('PC · Arrangement').first()).toBeVisible();
  });

  test('section titles match the spec', async ({ page }) => {
    await page.goto('/iExtend.html');
    await page.waitForSelector('[data-dc-section]');
    for (const t of ['Onboarding', 'Connected', 'Settings & errors', 'Floating toolbar variants']) {
      await expect(page.getByText(t).first()).toBeVisible();
    }
  });
});
