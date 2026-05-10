import XCTest
import SwiftUI
@testable import iExtendUI

final class BrandGradientTests: XCTestCase {

    func test_lockupSpacingRatioIsHalfMarkHeight() {
        XCTAssertEqual(BrandLockup.markToWordSpacingRatio, 0.5)
    }

    func test_compactSizes() {
        XCTAssertEqual(BrandLockup.Compact.markSize, 24)
        XCTAssertEqual(BrandLockup.Compact.wordSize, 14)
    }

    func test_heroSizes() {
        XCTAssertEqual(BrandLockup.Hero.markSize, 46)
        XCTAssertEqual(BrandLockup.Hero.wordSize, 22)
    }

    func test_heroIsLargerThanCompact() {
        XCTAssertGreaterThan(BrandLockup.Hero.markSize, BrandLockup.Compact.markSize)
        XCTAssertGreaterThan(BrandLockup.Hero.wordSize, BrandLockup.Compact.wordSize)
    }

    // The three brand-fold gradient values must be statically reachable.
    // (We can't introspect a LinearGradient's internal stops via public API,
    // so the test asserts the symbols compile and resolve.)
    func test_foldGradientsExist() {
        let _: LinearGradient = .brandFoldBlue
        let _: LinearGradient = .brandFoldIndigo
        let _: LinearGradient = .brandFoldPurple
    }
}
