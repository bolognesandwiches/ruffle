/**
 * These are properties and methods that Flash added to its `<embed>/<object>` tags.
 * These don't seem to be documented in full anywhere, and Ruffle adds them as we encounter some.
 * You are discouraged from using these, and they exist only to support legacy websites from decades ago.
 *
 * Extra methods or properties may appear at any time, due to `ExternalInterface.addCallback()`.
 * It may even overwrite existing methods or properties.
 */
export interface FlashAPI {
    /**
     * Returns the movies loaded process, in a percent from 0 to 100.
     * Ruffle may just return 0 or 100.
     *
     * @returns a value from 0 to 100, inclusive.
     */
    PercentLoaded(): number;

    /**
     * Gets a variable from the Flash movie using Flash Player 6 legacy interface.
     * Supports path-based access like "_level0.oDenizenManager" or "_level0.someVar".
     *
     * @param path The variable path to retrieve
     * @returns The variable value, or undefined if not found
     */
    GetVariable(path: string): any;

    /**
     * Sets a variable in the Flash movie using Flash Player 6 legacy interface.
     * Supports path-based access like "_level0.someVar".
     *
     * @param path The variable path to set
     * @param value The value to set
     * @returns true if successful, false otherwise
     */
    SetVariable(path: string, value: any): boolean;

    /**
     * Calls a function in the Flash movie using Flash Player 6 legacy interface.
     * Supports calling ActionScript methods with parameters.
     *
     * @param path The function path to call (e.g., "_root.oDenizenManager.createGateway")
     * @param args Array of arguments to pass to the function
     * @returns The function result, or undefined if call failed
     */
    CallFunction(path: string, args?: any[]): any;
}
