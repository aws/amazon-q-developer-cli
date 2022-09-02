// API.js is for internal use.
// It should only be used in contexts where we cannot use @withfig/api-bindings


let id = 0;
const handlers = {};

const sendMessageAsync = async (request) =>
    new Promise((resolve, reject) => {
      sendMessage(request, (response) => {
        resolve(response)
      })
     }
   );

const sendMessage = (request, handler) => {
  request.id = id++;
  
  if (handler) {
    handlers[request.id] = handler;
  }
  
  window.webkit.messageHandlers[fig.constants.jsonMessageHandler].postMessage(JSON.stringify(request))

}

const receivedMessage = (response) => {
  if (response.id === undefined) {
    return;
  }

  let handler = handlers[response.id]

  if (!handler) {
    return
  }

  handlers[response.id](response);

  delete handlers[response.id];
};

const makeRequest = async (message) => new Promise((resolve, reject) => {
  sendMessage(message, (response) => {
    if (response.error) {
      reject(response.error)
    } else {
      resolve(response)
    }
  })
})

document.addEventListener(fig.constants.jsonMessageRecieved, (event) => {
  const message = event.detail
  receivedMessage(message);
});
