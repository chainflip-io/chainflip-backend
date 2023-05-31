module.exports = {

runWithTimeout: (promise, millis) => {
    const timeout = new Promise((resolve, reject) =>
        setTimeout(
            () => reject("Timed out after " + millis + " ms."),
            millis));
    return Promise.race([
        promise,
        timeout
    ]);
}

};