import XCTest
@testable import iExtendUI

final class LogoGeometryTests: XCTestCase {

    func test_viewBoxIsSquare160() {
        XCTAssertEqual(LogoGeometry.viewBoxSize, 160)
    }

    func test_threePanelsExist() {
        XCTAssertEqual(LogoGeometry.panels.count, 3)
        XCTAssertEqual(LogoGeometry.panels.map { $0.name }, ["blue", "indigo", "purple"])
    }

    func test_eachPanelHasFourVertices() {
        for panel in LogoGeometry.panels {
            XCTAssertEqual(panel.vertices.count, 4, "panel \(panel.name)")
        }
    }

    func test_adjacentPanelsShareCreasePoints() {
        // The blue panel's right edge (vertices [1] and [2]) must equal
        // the indigo panel's left edge (vertices [0] and [3]).
        let blue   = LogoGeometry.panels[0]
        let indigo = LogoGeometry.panels[1]
        XCTAssertEqual(blue.vertices[1], indigo.vertices[0])
        XCTAssertEqual(blue.vertices[2], indigo.vertices[3])

        let purple = LogoGeometry.panels[2]
        XCTAssertEqual(indigo.vertices[1], purple.vertices[0])
        XCTAssertEqual(indigo.vertices[2], purple.vertices[3])
    }

    func test_creaseEndpointsMatchPanelEdges() {
        XCTAssertEqual(LogoGeometry.creases.count, 2)
        let blue   = LogoGeometry.panels[0]
        let indigo = LogoGeometry.panels[1]
        XCTAssertEqual(LogoGeometry.creases[0].start, blue.vertices[1])
        XCTAssertEqual(LogoGeometry.creases[0].end,   blue.vertices[2])
        XCTAssertEqual(LogoGeometry.creases[1].start, indigo.vertices[1])
        XCTAssertEqual(LogoGeometry.creases[1].end,   indigo.vertices[2])
    }
}
